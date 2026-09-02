import { describe, expect, it } from 'vitest'
import { getModelAbilityTags } from './modelCapabilities'

describe('getModelAbilityTags', () => {
  it('prioritizes image generation over the supporting vision capability', () => {
    expect(getModelAbilityTags('gpt-image-1', {
      capabilities: { imageGeneration: true, vision: true },
    })).toEqual(['image'])
  })

  it('identifies embedding models without adding a general tag', () => {
    expect(getModelAbilityTags('text-embedding-3-small', {
      capabilities: { embedding: true },
    })).toEqual(['embedding'])
  })

  it('exposes code and reasoning for coding models', () => {
    expect(getModelAbilityTags('qwen3-coder', {
      capabilities: { reasoning: true },
    })).toEqual(['code', 'reasoning'])
  })

  it('keeps complementary reasoning and vision labels for multimodal models', () => {
    expect(getModelAbilityTags('gpt-5.6', {
      capabilities: { reasoning: true, vision: true },
    })).toEqual(['reasoning', 'vision'])
  })

  it('falls back to one general label for unknown models', () => {
    expect(getModelAbilityTags('my-custom-model', {})).toEqual(['general'])
  })
})
