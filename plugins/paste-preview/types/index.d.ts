export type PastedImage = { number: number; path: string }
export type Platform = 'linux' | 'mac' | 'windows'
export type LabelMessage = { number: number }

declare module 'claude-code' {
  interface PluginState {
    'paste-preview': { images: PastedImage[] }
  }
}
