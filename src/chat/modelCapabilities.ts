import { resolveModelInfo } from '../data/modelMatching'
import type { ModelInfo } from '../api/tauri'

export type ModelAbilityTag = 'reasoning' | 'vision' | 'image' | 'embedding' | 'code' | 'general'

/**
 * Returns a deliberately small set of user-facing ability tags for a model row.
 * Image and embedding models are specialised endpoints, so they get one tag;
 * chat models may expose complementary reasoning/vision tags.
 */
export function getModelAbilityTags(model: string, info?: ModelInfo): ModelAbilityTag[] {
  const capabilities = info?.capabilities ?? {}
  if (capabilities.imageGeneration) return ['image']
  if (capabilities.embedding) return ['embedding']

  const tags: ModelAbilityTag[] = []
  if (isCodeModel(model)) tags.push('code')
  if (capabilities.reasoning) tags.push('reasoning')
  if (capabilities.vision) tags.push('vision')
  return tags.length > 0 ? tags : ['general']
}

/** Resolve built-in metadata while preserving user-provided provider overrides. */
export function getModelAbilityTagsForProvider(
  model: string,
  modelOverrides?: Record<string, ModelInfo>,
): ModelAbilityTag[] {
  return getModelAbilityTags(model, resolveModelInfo(model, modelOverrides))
}

function isCodeModel(model: string): boolean {
  return /(?:^|[-_/])(?:code|coder|codex|codestral|devstral)(?:[-_/]|$)/i.test(model)
    || /(?:^|[-_/])composer(?:[-_/]|$)/i.test(model)
}
