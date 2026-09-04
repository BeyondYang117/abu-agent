import { describe, expect, it } from 'vitest'
import type { ModelProvider } from '../api/tauri'
import { classifySmartModelTask, selectSmartModel } from './smartModelRouting'
import type { ModelRoutingPolicy } from './modelRoutingPolicy'

function provider(models: string[]): ModelProvider {
  return {
    id: 'cloud',
    name: 'Cloud',
    apiKeys: [],
    baseUrl: '',
    availableModels: models,
    enabledModels: models,
    enabled: true,
    apiFormat: 'openai_chat',
  }
}

describe('smart model routing', () => {
  it('recognizes common user goals', () => {
    expect(classifySmartModelTask('写一篇江南雨夜的悬疑小说', false)).toBe('creative')
    expect(classifySmartModelTask('帮我调试这个 TypeScript 组件', false)).toBe('coding')
    expect(classifySmartModelTask('总结这张截图', true)).toBe('vision')
  })

  it('prefers a premium creative model in quality mode', () => {
    const route = selectSmartModel({
      providers: [provider(['claude-haiku-4-5', 'claude-opus-4-5', 'gpt-5.6-luna'])],
      content: '写一篇短篇小说',
      hasImage: false,
      quality: 'quality',
    })
    expect(route).toMatchObject({ model: 'claude-opus-4-5', task: 'creative' })
  })

  it('prefers a lightweight model in fast mode', () => {
    const route = selectSmartModel({
      providers: [provider(['claude-opus-4-5', 'claude-haiku-4-5'])],
      content: '把这句话改得更简洁',
      hasImage: false,
      quality: 'fast',
    })
    expect(route?.model).toBe('claude-haiku-4-5')
  })

  it('only routes image input to a vision-capable model', () => {
    const models = provider(['text-private', 'gpt-4o'])
    models.modelOverrides = {
      'text-private': { capabilities: { vision: false } },
    }
    const route = selectSmartModel({
      providers: [models],
      content: '这是什么？',
      hasImage: true,
      quality: 'balanced',
    })
    expect(route?.model).toBe('gpt-4o')
  })

  it('uses the published policy before local model-name heuristics', () => {
    const policy: ModelRoutingPolicy = {
      version: 7, updated_at: 1, recommended: {}, fallbacks: {},
      rules: [
        { model: 'plain-a', enabled: true, healthy: true, tiers: ['quality'], task_scores: { general: 10, creative: 99, coding: 10, reasoning: 10, vision: 0 }, capabilities: {}, multilingual: true, cost_level: 'high', priority: 0, fallback_priority: 1, rollout_percent: 100, updated_at: 1, version: 7 },
        { model: 'claude-opus-premium', enabled: true, healthy: true, tiers: ['quality'], task_scores: { general: 90, creative: 20, coding: 90, reasoning: 90, vision: 0 }, capabilities: {}, multilingual: true, cost_level: 'high', priority: 0, fallback_priority: 2, rollout_percent: 100, updated_at: 1, version: 7 },
      ],
    }
    const route = selectSmartModel({ providers: [provider(['plain-a', 'claude-opus-premium'])], content: '写一篇小说', hasImage: false, quality: 'quality', policy })
    expect(route).toMatchObject({ model: 'plain-a', policyVersion: 7 })
  })

  it('enforces vision and manual mode remains outside the router', () => {
    const policy: ModelRoutingPolicy = {
      version: 8, updated_at: 1, recommended: {}, fallbacks: {},
      rules: [
        { model: 'text', enabled: true, healthy: true, tiers: ['balanced'], task_scores: { general: 100, creative: 100, coding: 100, reasoning: 100, vision: 100 }, capabilities: {}, multilingual: true, cost_level: 'low', priority: 100, fallback_priority: 2, rollout_percent: 100, updated_at: 1, version: 8 },
        { model: 'eyes', enabled: true, healthy: true, tiers: ['balanced'], task_scores: { general: 1, creative: 1, coding: 1, reasoning: 1, vision: 1 }, capabilities: { vision: true }, multilingual: true, cost_level: 'low', priority: 0, fallback_priority: 1, rollout_percent: 100, updated_at: 1, version: 8 },
      ],
    }
    const route = selectSmartModel({ providers: [provider(['text', 'eyes'])], content: '看看', hasImage: true, quality: 'balanced', policy })
    expect(route?.model).toBe('eyes')
    expect(route?.candidates?.find((item) => item.model === 'text')?.eliminatedReasons).toContain('vision_required')
  })

  it('uses the server fallback order after selecting the highest score', () => {
    const policy: ModelRoutingPolicy = {
      version: 9, updated_at: 1, recommended: {}, fallbacks: { balanced: ['backup-b', 'backup-a'] },
      rules: [
        { model: 'winner', enabled: true, healthy: true, tiers: ['balanced'], task_scores: { general: 99, creative: 99, coding: 99, reasoning: 99, vision: 0 }, capabilities: {}, multilingual: true, cost_level: 'medium', priority: 0, fallback_priority: 0, rollout_percent: 100, updated_at: 1, version: 9 },
        { model: 'backup-a', enabled: true, healthy: true, tiers: ['balanced'], task_scores: { general: 80, creative: 80, coding: 80, reasoning: 80, vision: 0 }, capabilities: {}, multilingual: true, cost_level: 'medium', priority: 0, fallback_priority: 20, rollout_percent: 100, updated_at: 1, version: 9 },
        { model: 'backup-b', enabled: true, healthy: true, tiers: ['balanced'], task_scores: { general: 70, creative: 70, coding: 70, reasoning: 70, vision: 0 }, capabilities: {}, multilingual: true, cost_level: 'medium', priority: 0, fallback_priority: 10, rollout_percent: 100, updated_at: 1, version: 9 },
      ],
    }
    const route = selectSmartModel({ providers: [provider(['winner', 'backup-a', 'backup-b'])], content: 'hello', hasImage: false, quality: 'balanced', policy })
    expect(route?.model).toBe('winner')
    expect(route?.fallbacks?.map((item) => item.model)).toEqual(['backup-b', 'backup-a'])
  })
})
