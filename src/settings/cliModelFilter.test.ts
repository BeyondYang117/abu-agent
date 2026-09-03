import { describe, expect, it } from 'vitest'
import { filterCliModels } from './cliModelFilter'

describe('filterCliModels', () => {
  const catalog = [
    'gpt-5.5', 'codex-mini-latest', 'o3-mini', 'claude-opus-4-1',
    'gemini-2.5-pro', 'text-embedding-3-large', 'dall-e-3', 'gpt-5.5',
  ]

  it('keeps Codex/GPT reasoning models only for Codex', () => {
    expect(filterCliModels('codex', catalog)).toEqual(['gpt-5.5', 'codex-mini-latest', 'o3-mini'])
  })

  it('keeps the CLI family for Claude and Gemini', () => {
    expect(filterCliModels('claude', catalog)).toEqual(['claude-opus-4-1'])
    expect(filterCliModels('gemini', catalog)).toEqual(['gemini-2.5-pro'])
  })

  it('removes non-chat models and caps generic CLI suggestions', () => {
    const many = Array.from({ length: 60 }, (_, index) => `model-${index}`)
    expect(filterCliModels('opencode', [...many, 'model-0', 'text-embedding-3-small'])).toHaveLength(24)
    expect(filterCliModels('opencode', [...many, 'model-0'])).toEqual(many.slice(0, 24))
  })

  it('falls back to custom chat model ids when no family name matches', () => {
    expect(filterCliModels('codex', ['company-coder', 'text-embedding-3-small'])).toEqual(['company-coder'])
    expect(filterCliModels('codex', ['claude-opus-4-1', 'claude-sonnet-4-6'])).toEqual([])
  })
})
