import { describe, expect, it } from 'vitest'
import { hasModelInfo, matchModel, resolveModelInfo } from './modelMatching'

describe('matchModel', () => {
  it('returns null for blank model names', () => {
    expect(matchModel('')).toBeNull()
    expect(matchModel('   ')).toBeNull()
  })

  it('matches known models by exact id', () => {
    const info = matchModel('gpt-4o')
    expect(info).not.toBeNull()
    expect(info?.displayName).toBeTruthy()
    expect(info?.contextWindow).toBeGreaterThan(0)
  })

  it('strips OpenRouter-style provider prefix before matching', () => {
    const direct = matchModel('gpt-4o')
    const prefixed = matchModel('openai/gpt-4o')
    expect(prefixed).toEqual(direct)
  })

  it('returns null for unknown models', () => {
    expect(matchModel('totally-unknown-model-xyz-9999')).toBeNull()
  })

  it('recognizes image generation model naming patterns', () => {
    const info = matchModel('dall-e-3')
    expect(info?.capabilities?.imageGeneration).toBe(true)
  })
})

describe('resolveModelInfo', () => {
  it('merges database defaults with user overrides', () => {
    const resolved = resolveModelInfo('gpt-4o', {
      'gpt-4o': {
        displayName: 'Custom GPT-4o',
      },
    })
    expect(resolved.displayName).toBe('Custom GPT-4o')
    expect(resolved.contextWindow).toBeGreaterThan(0)
  })

  it('returns override-only info when database has no match', () => {
    const resolved = resolveModelInfo('custom-local-model', {
      'custom-local-model': {
        displayName: 'Local',
        contextWindow: 8192,
      },
    })
    expect(resolved.displayName).toBe('Local')
    expect(resolved.contextWindow).toBe(8192)
  })
})

describe('hasModelInfo', () => {
  it('returns true when database or overrides provide info', () => {
    expect(hasModelInfo('gpt-4o')).toBe(true)
    expect(hasModelInfo('unknown', { unknown: { displayName: 'X' } })).toBe(true)
    expect(hasModelInfo('unknown')).toBe(false)
  })
})
