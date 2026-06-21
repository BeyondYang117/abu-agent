import { describe, expect, it } from 'vitest'
import { _matchGlyphForTest as matchGlyph } from './ModelIcon'

describe('ModelIcon model→brand mapping', () => {
  it('matches common model families', () => {
    const cases = [
      'gpt-4o', 'o3-mini', 'claude-3-5-sonnet', 'gemini-2.0-flash', 'gemma-2',
      'deepseek-chat', 'qwen-max', 'grok-3', 'kimi-k2', 'moonshot-v1-8k',
      'glm-4', 'mistral-large', 'llama-3.1-70b', 'yi-large', 'doubao-pro',
      'ernie-4.0', 'minimax-abab6', 'command-r', 'phi-3-medium', 'step-1v',
    ]
    for (const id of cases) {
      expect(matchGlyph(id), `${id} should resolve a brand`).not.toBeNull()
    }
  })

  it('is case-insensitive', () => {
    expect(matchGlyph('GPT-4O')).toBe(matchGlyph('gpt-4o'))
  })

  it('does not misfire on substrings (gemma ≠ gemini, yi only as token)', () => {
    expect(matchGlyph('mayis-model')).toBeNull() // "yi" inside a word must not match
  })

  it('returns null for unknown models', () => {
    expect(matchGlyph('totally-made-up-model')).toBeNull()
    expect(matchGlyph('')).toBeNull()
  })
})
