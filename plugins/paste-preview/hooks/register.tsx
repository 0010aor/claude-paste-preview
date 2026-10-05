import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { LabelMessage, PastedImage, Platform } from '../types'

const DRAFT_POLL_MS = 300
const IMAGES_DIR_RETRY_MS = 5000
const OPEN_TIMEOUT_MS = 10000
const HELPER = 'claude-paste-helper'

const images = atom({ plugin: 'paste-preview', key: 'images' } as const, [])

type Env = Partial<Record<'CLAUDE_CODE_TMPDIR' | 'TMPDIR' | 'TMP' | 'TEMP' | 'HOME' | 'LOCALAPPDATA', string>>

export function imageNumbers(text: string): number[] {
  return [...new Set([...text.matchAll(/\[Image #(\d+)\]/g)].map(match => Number(match[1])))]
}

export function claudeTmpRoot(env: Env, platform: Platform, uid: string) {
  const systemTmp = platform === 'windows' ? env.TEMP || env.TMP : env.TMPDIR || env.TMP || env.TEMP || '/tmp'
  const base = env.CLAUDE_CODE_TMPDIR || systemTmp || '.'
  return `${base.replace(/[\\/]+$/, '')}/claude-${platform === 'windows' ? '0' : uid}`
}

export function projectDirName(projectRoot: string) {
  return projectRoot.replace(/[^a-zA-Z0-9]/g, '-')
}

export function hasNewPaste(pasted: readonly PastedImage[], alreadyPreviewed: ReadonlySet<string>) {
  return pasted.some(image => !alreadyPreviewed.has(image.path))
}

export function helperCandidates(platform: Platform, env: Env): string[] {
  if (platform === 'windows') {
    const installed = env.LOCALAPPDATA ? [`${env.LOCALAPPDATA}\\claude-paste-preview\\${HELPER}.exe`] : []
    return [...installed, HELPER]
  }
  return [HELPER, ...(env.HOME ? [`${env.HOME}/.local/bin/${HELPER}`] : [])]
}

export function previewCommand(
  platform: Platform,
  paths: readonly string[],
  helper: string | undefined,
): string[] | undefined {
  if (paths.length === 0) return undefined
  if (helper) return [helper, 'preview', ...paths]
  if (platform === 'mac') return ['qlmanage', '-p', ...paths]
  return undefined
}

export function openCommands(platform: Platform, path: string): string[][] {
  if (platform === 'mac') return [['open', path]]
  if (platform === 'windows') return [['explorer.exe', path.replaceAll('/', '\\')]]
  return [
    ['xdg-open', path],
    ['gio', 'open', path],
  ]
}

type Session = {
  platform: Platform
  tmpRoot: string
  helper: string | undefined
}

let session: Session | undefined
let imagesDir: { sessionId: string; dir?: string; retryAt: number } | undefined
const knownImages = new Set<string>()
const previewedOnPaste = new Set<string>()
let lastDraft: string | undefined
let isPreviewOpen = false
let preview: { key: string; stop: () => void } | undefined
let isSyncing = false
let bandShowsImages = ''

async function detectPlatform($: EngineInterface): Promise<Platform> {
  if ((await $.env.get('OS')) === 'Windows_NT') return 'windows'
  const uname = await $.process.run(['uname', '-s']).catch(() => undefined)
  return uname?.stdout.trim() === 'Darwin' ? 'mac' : 'linux'
}

async function readEnv($: EngineInterface): Promise<Env> {
  const [CLAUDE_CODE_TMPDIR, TMPDIR, TMP, TEMP, HOME, LOCALAPPDATA] = await Promise.all([
    $.env.get('CLAUDE_CODE_TMPDIR'),
    $.env.get('TMPDIR'),
    $.env.get('TMP'),
    $.env.get('TEMP'),
    $.env.get('HOME'),
    $.env.get('LOCALAPPDATA'),
  ])
  return { CLAUDE_CODE_TMPDIR, TMPDIR, TMP, TEMP, HOME, LOCALAPPDATA }
}

async function findHelper($: EngineInterface, candidates: string[]) {
  for (const candidate of candidates) {
    const version = await $.process.run([candidate, '--version'], { timeoutMs: 5000 }).catch(() => undefined)
    if (version?.exitCode === 0) return candidate
  }
  return undefined
}

async function startSession($: EngineInterface): Promise<Session> {
  const platform = await detectPlatform($)
  const env = await readEnv($)
  const [uid, helper] = await Promise.all([
    platform === 'windows' ? '0' : $.process.run(['id', '-u']).then(({ stdout }) => stdout.trim(), () => '0'),
    findHelper($, helperCandidates(platform, env)),
  ])
  return { platform, tmpRoot: claudeTmpRoot(env, platform, uid), helper }
}

async function scanForImagesDir($: EngineInterface, tmpRoot: string, sessionId: string) {
  const projectGuess = `${tmpRoot}/${projectDirName(await $.session.root())}/${sessionId}/images`
  if (await $.fs.exists(projectGuess)) return projectGuess
  for (const entry of await $.fs.list(tmpRoot).catch(() => [])) {
    const dir = `${tmpRoot}/${entry.name}/${sessionId}/images`
    if (entry.kind === 'dir' && (await $.fs.exists(dir))) return dir
  }
  return undefined
}

async function findImagesDir($: EngineInterface, tmpRoot: string): Promise<string | undefined> {
  const sessionId = await $.session.id()
  const now = await $.clock.now()
  if (imagesDir?.sessionId === sessionId && (imagesDir.dir || now < imagesDir.retryAt)) return imagesDir.dir
  const dir = await scanForImagesDir($, tmpRoot, sessionId)
  imagesDir = { sessionId, dir, retryAt: now + IMAGES_DIR_RETRY_MS }
  return dir
}

async function draftImages($: EngineInterface, text: string): Promise<PastedImage[]> {
  const numbers = imageNumbers(text)
  if (numbers.length === 0 || !session) return []
  const dir = await findImagesDir($, session.tmpRoot)
  if (!dir) return []
  const found: PastedImage[] = []
  for (const number of numbers) {
    const path = `${dir}/${number}.png`
    if (knownImages.has(path) || (await $.fs.exists(path))) {
      knownImages.add(path)
      found.push({ number, path })
    }
  }
  return found
}

function imagesKey(pasted: readonly PastedImage[]) {
  return pasted.map(image => image.path).join('\n')
}

function runUntilStopped($: EngineInterface, argv: string[], onExit: () => void) {
  const child = $.process.spawn({ argv })
  void (async () => {
    try {
      for await (const _ of child) {
      }
    } catch {}
    onExit()
  })()
  return () => void child.return(undefined as never).catch(() => {})
}

function syncPreview($: EngineInterface, pasted: readonly PastedImage[]) {
  const paths = isPreviewOpen ? pasted.map(image => image.path) : []
  const argv = session ? previewCommand(session.platform, paths, session.helper) : undefined
  const key = argv?.join('\0') ?? ''
  if ((preview?.key ?? '') === key) return
  preview?.stop()
  if (!argv) {
    preview = undefined
    return
  }
  const started = {
    key,
    stop: runUntilStopped($, argv, () => {
      if (preview !== started) return
      preview = undefined
      isPreviewOpen = false
    }),
  }
  preview = started
}

async function openInViewer($: EngineInterface, path: string) {
  if (!session) return
  for (const argv of openCommands(session.platform, path)) {
    const result = await $.process.run(argv, { timeoutMs: OPEN_TIMEOUT_MS }).catch(() => undefined)
    if (result && (result.exitCode === 0 || session.platform === 'windows')) return
  }
}

async function handleLabelClick($: EngineInterface, { number }: LabelMessage) {
  const pasted = await read($, images)
  const image = pasted.find(candidate => candidate.number === number)
  if (!image || !session) return
  if (!previewCommand(session.platform, [image.path], session.helper)) {
    await openInViewer($, image.path)
    return
  }
  isPreviewOpen = !isPreviewOpen
  syncPreview($, pasted)
}

function openPreviewOnNewPaste(pasted: readonly PastedImage[]) {
  if (hasNewPaste(pasted, previewedOnPaste)) isPreviewOpen = true
  for (const image of pasted) previewedOnPaste.add(image.path)
}

async function syncWithDraft($: EngineInterface, draft?: string) {
  if (isSyncing) return
  isSyncing = true
  try {
    const text = draft ?? (await $.prompt.read()).text
    const current = await read($, images)
    const isSettled = text === lastDraft && current.length === imageNumbers(text).length
    if (isSettled) return
    lastDraft = text
    const pasted = await draftImages($, text)
    if (imagesKey(pasted) !== imagesKey(current)) await update($, images, () => pasted)
    openPreviewOnNewPaste(pasted)
    syncPreview($, pasted)
  } finally {
    isSyncing = false
  }
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    const started = next(e)
    session = await startSession($)
    $.clock.every(DRAFT_POLL_MS, () => syncWithDraft($))
    return started
  })

  on('prompt.edit', async ($, e, next) => {
    const result = await next(e)
    void syncWithDraft($, result.text)
    return result
  })

  on('prompt.submit', async ($, e, next) => {
    isPreviewOpen = false
    lastDraft = undefined
    await update($, images, () => [])
    syncPreview($, [])
    return next(e)
  })

  on('ui.message', { module: 'hooks/imageLabels.tsx' }, async ($, e) => {
    await handleLabelClick($, e.data as LabelMessage)
    return {}
  })

  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    const pasted = await read($, images)
    const shown = e.surface === 'terminal' && !e.props.hasSurvey ? imagesKey(pasted) : ''
    if (bandShowsImages !== shown) {
      bandShowsImages = shown
      $.ui.invalidate('ui.render')
    }
    if (!shown || e.surface !== 'terminal') return next(e)

    const { Box, Client } = $.ui.resolve(e)
    const beneath = await next(e)
    return (
      <Box flexDirection="column">
        <Client key="labels" module="./imageLabels.tsx" props={{ numbers: pasted.map(image => image.number) }} />
        {beneath}
      </Box>
    )
  })

  on('ui.render', { component: 'PromptHint' }, async ($, e, next) => {
    const pasted = await read($, images)
    if (e.surface !== 'terminal' || pasted.length === 0 || bandShowsImages === imagesKey(pasted)) return next(e)

    const { Box, Client } = $.ui.resolve(e)
    const beneath = await next(e)
    return (
      <Box flexDirection="row">
        <Client key="labels" module="./imageLabels.tsx" props={{ numbers: pasted.map(image => image.number) }} />
        {beneath}
      </Box>
    )
  })
}
