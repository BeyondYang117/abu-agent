/**
 * @vitest-environment jsdom
 */
import { describe, expect, it } from 'vitest'
import { nextThemeMode, resolveThemeIconMode } from './themeMode'

describe('theme shortcut', () => {
  it('cycles light, dark, and system modes', () => {
    expect(nextThemeMode('light')).toBe('dark')
    expect(nextThemeMode('dark')).toBe('system')
    expect(nextThemeMode('system')).toBe('light')
  })

  it('resolves system mode from the document dark class', () => {
    document.documentElement.classList.remove('dark')
    expect(resolveThemeIconMode('system')).toBe('light')
    document.documentElement.classList.add('dark')
    expect(resolveThemeIconMode('system')).toBe('dark')
  })
})
