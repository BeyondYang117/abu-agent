export type ThemeMode = 'system' | 'light' | 'dark'

export function nextThemeMode(mode: ThemeMode): ThemeMode {
  if (mode === 'light') return 'dark'
  if (mode === 'dark') return 'system'
  return 'light'
}

export function resolveThemeIconMode(mode: ThemeMode): 'light' | 'dark' {
  if (mode !== 'system') return mode
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light'
}
