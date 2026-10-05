import type { RenderElement } from 'claude-code'
import { describe, expect, test } from 'claude-code/testing'

import { labelIndexAt, labelText } from './labelLayout'
import {
  claudeTmpRoot,
  hasNewPaste,
  helperCandidates,
  imageNumbers,
  openCommands,
  previewCommand,
  projectDirName,
} from './register'

describe('pasted image tags', () => {
  test('finds each tag once, in order', () => {
    expect(imageNumbers('compare [Image #1] with [Image #12] and [Image #1]')).toEqual([1, 12])
  })

  test('ignores look-alikes', () => {
    expect(imageNumbers('[Image #x] [image #2] [Image 3] Image #4')).toEqual([])
    expect(imageNumbers('')).toEqual([])
  })
})

describe('opening the preview on paste', () => {
  const pasted = [
    { number: 1, path: '/i/1.png' },
    { number: 2, path: '/i/2.png' },
  ]

  test('opens for an image not yet previewed', () => {
    expect(hasNewPaste(pasted, new Set())).toBe(true)
    expect(hasNewPaste(pasted, new Set(['/i/2.png']))).toBe(true)
  })

  test('stays closed once every image has been previewed', () => {
    expect(hasNewPaste(pasted, new Set(['/i/1.png', '/i/2.png']))).toBe(false)
    expect(hasNewPaste([], new Set())).toBe(false)
  })
})

describe("Claude Code's temp folder", () => {
  test('is the system temp folder plus claude-<uid> on Linux', () => {
    expect(claudeTmpRoot({}, 'linux', '1000')).toBe('/tmp/claude-1000')
    expect(claudeTmpRoot({ TMPDIR: '/var/tmp/' }, 'linux', '1000')).toBe('/var/tmp/claude-1000')
  })

  test('follows TMPDIR on macOS', () => {
    expect(claudeTmpRoot({ TMPDIR: '/var/folders/ab/cd/T/' }, 'mac', '501')).toBe('/var/folders/ab/cd/T/claude-501')
  })

  test('uses TEMP and uid 0 on Windows', () => {
    expect(claudeTmpRoot({ TEMP: 'C:\\Users\\me\\AppData\\Local\\Temp' }, 'windows', '1000')).toBe(
      'C:\\Users\\me\\AppData\\Local\\Temp/claude-0',
    )
  })

  test('prefers CLAUDE_CODE_TMPDIR everywhere', () => {
    expect(claudeTmpRoot({ CLAUDE_CODE_TMPDIR: '/scratch', TMPDIR: '/var/tmp' }, 'linux', '7')).toBe(
      '/scratch/claude-7',
    )
  })

  test('names the project folder the way Claude Code does', () => {
    expect(projectDirName('/home/me/my.project_x')).toBe('-home-me-my-project-x')
  })
})

describe('per-platform commands', () => {
  test('previews every image with the helper wherever it is installed', () => {
    for (const platform of ['linux', 'mac', 'windows'] as const) {
      expect(previewCommand(platform, ['/i/1.png', '/i/2.png'], '/h/claude-paste-helper')).toEqual([
        '/h/claude-paste-helper',
        'preview',
        '/i/1.png',
        '/i/2.png',
      ])
    }
    expect(previewCommand('linux', [], '/h/claude-paste-helper')).toBeUndefined()
  })

  test('falls back to Quick Look on macOS and to nothing elsewhere without the helper', () => {
    expect(previewCommand('mac', ['/i/1.png'], undefined)).toEqual(['qlmanage', '-p', '/i/1.png'])
    expect(previewCommand('linux', ['/i/1.png'], undefined)).toBeUndefined()
    expect(previewCommand('windows', ['C:/t/1.png'], undefined)).toBeUndefined()
  })

  test('looks for the helper on PATH and where the installer puts it', () => {
    expect(helperCandidates('linux', { HOME: '/home/me' })).toEqual([
      'claude-paste-helper',
      '/home/me/.local/bin/claude-paste-helper',
    ])
    expect(helperCandidates('mac', {})).toEqual(['claude-paste-helper'])
    expect(helperCandidates('windows', { LOCALAPPDATA: 'C:\\Users\\me\\AppData\\Local' })).toEqual([
      'C:\\Users\\me\\AppData\\Local\\claude-paste-preview\\claude-paste-helper.exe',
      'claude-paste-helper',
    ])
  })

  test('opens with the desktop default viewer', () => {
    expect(openCommands('linux', '/i/1.png')).toEqual([
      ['xdg-open', '/i/1.png'],
      ['gio', 'open', '/i/1.png'],
    ])
    expect(openCommands('mac', '/i/1.png')).toEqual([['open', '/i/1.png']])
    expect(openCommands('windows', 'C:\\t/claude-0/p/s/images/1.png')).toEqual([
      ['explorer.exe', 'C:\\t\\claude-0\\p\\s\\images\\1.png'],
    ])
  })
})

describe('label hit-testing', () => {
  test('maps a column to the label under it, its trailing gap included', () => {
    const numbers = [1, 12]
    const firstWidth = labelText(1).length + 2
    expect(labelIndexAt(0, numbers)).toBe(0)
    expect(labelIndexAt(firstWidth - 1, numbers)).toBe(0)
    expect(labelIndexAt(firstWidth, numbers)).toBe(1)
    expect(labelIndexAt(firstWidth + labelText(12).length + 2, numbers)).toBeUndefined()
    expect(labelIndexAt(-1, numbers)).toBeUndefined()
    expect(labelIndexAt(0, [])).toBeUndefined()
  })
})

const BAND = { plugin: 'paste-preview', surface: 'terminal', component: 'AbovePrompt' } as const
const BAND_PROPS = {
  hasSurvey: false,
  isWorking: false,
  maxRows: 10,
  bodyColumns: 80,
  scroll: { offset: 0, bodyRows: 9, contentRows: 1 },
  view: {},
}

describe('the band above the prompt', () => {
  test('leaves the band to whatever is beneath while nothing is pasted', async ($, on) => {
    on('ui.render', { component: 'AbovePrompt' }, () => h('Text', {}, 'beneath') as RenderElement)
    const band = await $.ui.mount({ ...BAND, props: BAND_PROPS as never })
    expect(await band.find({ text: 'beneath' })).toBeDefined()
    expect(await band.find({ text: 'Image #' })).toBeUndefined()
    await band.unmount()
  })
})
