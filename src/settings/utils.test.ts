import { describe, expect, it } from 'vitest'
import { formatHotkey, modelPairValue, parseModelPairValue } from './utils'

describe('formatHotkey', () => {
  it('renders macOS modifier glyphs', () => {
    expect(formatHotkey('CommandOrControl+Shift+T', 'macos')).toEqual(['⌘', '⇧', 'T'])
    expect(formatHotkey('Alt+Space', 'macos')).toEqual(['⌥', 'Space'])
  })

  it('renders Windows modifier labels', () => {
    expect(formatHotkey('CommandOrControl+Shift+T', 'windows')).toEqual(['Ctrl', 'Shift', 'T'])
    expect(formatHotkey('Alt+Space', 'windows')).toEqual(['Alt', 'Space'])
  })

  it('maps arrow keys to symbols on macOS', () => {
    expect(formatHotkey('ArrowUp', 'macos')).toEqual(['↑'])
    expect(formatHotkey('ArrowDown', 'macos')).toEqual(['↓'])
  })
})

describe('modelPairValue', () => {
  it('serializes provider and model as JSON array', () => {
    expect(modelPairValue('openai', 'gpt-4o')).toBe('["openai","gpt-4o"]')
  })
})

describe('parseModelPairValue', () => {
  it('parses JSON array values', () => {
    expect(parseModelPairValue('["openai","gpt-4o"]')).toEqual(['openai', 'gpt-4o'])
  })

  it('parses legacy provider:model values', () => {
    expect(parseModelPairValue('openai:gpt-4o')).toEqual(['openai', 'gpt-4o'])
  })

  it('returns model-less pair when no separator exists', () => {
    expect(parseModelPairValue('openai')).toEqual(['openai', ''])
  })
})
