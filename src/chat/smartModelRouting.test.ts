import { describe, expect, it } from 'vitest'
import type { ModelProvider } from '../api/tauri'
import { classifySmartModelTask, selectSmartModel } from './smartModelRouting'

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
})
