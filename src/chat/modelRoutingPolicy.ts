import { api, isTauriRuntime } from '../api/tauri'
import type { SmartModelQuality, SmartModelTask } from './smartModelRouting'

export type ModelRoutingRule = {
  model: string
  provider?: string
  model_group?: string
  enabled: boolean
  tiers: SmartModelQuality[]
  task_scores: Record<SmartModelTask, number>
  capabilities: {
    vision?: boolean
    reasoning?: boolean
    function_calling?: boolean
    streaming?: boolean
    embedding?: boolean
    image_generation?: boolean
  }
  multilingual: boolean
  cost_level: 'low' | 'medium' | 'high'
  priority: number
  fallback_priority: number
  rollout_percent: number
  notes?: string
  updated_at: number
  version: number
  healthy: boolean
  health_reason?: string
}

export type ModelRoutingPolicy = {
  version: number
  updated_at: number
  rules: ModelRoutingRule[]
  recommended: Partial<Record<SmartModelQuality, string>>
  fallbacks: Partial<Record<SmartModelQuality, string[]>>
  task_mappings?: Array<{ task: SmartModelTask; keywords: string[] }>
}

const MAX_POLICY_AGE_MS = 30 * 60 * 1000
let currentCache: { version: number; fetched_at: number; payload: ModelRoutingPolicy } | null = null
let syncPromise: Promise<ModelRoutingPolicy | null> | null = null

function parsePolicyCache(value: unknown): typeof currentCache {
  if (!value || typeof value !== 'object') return null
  const cache = value as { version?: unknown; fetched_at?: unknown; payload?: unknown }
  const payload = cache.payload as Partial<ModelRoutingPolicy> | undefined
  if (!payload || !Array.isArray(payload.rules) || typeof cache.version !== 'number' || cache.version <= 0) return null
  return {
    version: cache.version,
    fetched_at: typeof cache.fetched_at === 'number' ? cache.fetched_at : 0,
    payload: {
      version: cache.version,
      updated_at: typeof payload.updated_at === 'number' ? payload.updated_at : 0,
      rules: payload.rules as ModelRoutingRule[],
      recommended: payload.recommended && typeof payload.recommended === 'object' ? payload.recommended : {},
      fallbacks: payload.fallbacks && typeof payload.fallbacks === 'object' ? payload.fallbacks : {},
      task_mappings: Array.isArray(payload.task_mappings) ? payload.task_mappings : [],
    },
  }
}

export async function loadCachedModelRoutingPolicy(): Promise<ModelRoutingPolicy | null> {
  if (currentCache) return currentCache.payload
  if (!isTauriRuntime()) return null
  try {
    currentCache = parsePolicyCache(await api.abuApiGetCachedModelRoutingPolicy())
  } catch {
    currentCache = null
  }
  return currentCache?.payload ?? null
}

export async function syncModelRoutingPolicy(): Promise<ModelRoutingPolicy | null> {
  if (!isTauriRuntime()) return currentCache?.payload ?? null
  if (syncPromise) return syncPromise
  syncPromise = api.abuApiSyncModelRoutingPolicy()
    .then((value) => {
      currentCache = parsePolicyCache(value)
      return currentCache?.payload ?? null
    })
    .catch((error) => {
      console.warn('Model routing policy sync failed; keeping cached policy:', error)
      return currentCache?.payload ?? null
    })
    .finally(() => { syncPromise = null })
  return syncPromise
}

export async function getModelRoutingPolicy(refreshIfStale = true): Promise<ModelRoutingPolicy | null> {
  const cached = await loadCachedModelRoutingPolicy()
  const stale = !currentCache || Date.now() - currentCache.fetched_at * 1000 >= MAX_POLICY_AGE_MS
  if (refreshIfStale && stale) return syncModelRoutingPolicy()
  return cached
}

export function findModelRoutingRule(
  policy: ModelRoutingPolicy | null,
  providerId: string,
  model: string,
): ModelRoutingRule | undefined {
  return policy?.rules.find((rule) => rule.model === model && (!rule.provider || providerId === 'abu-api-relay' || rule.provider === providerId))
}

export function policyModelLabels(rule: ModelRoutingRule, language: 'zh' | 'en' = 'zh'): string[] {
  const tierLabel = { fast: language === 'zh' ? '快速' : 'Fast', balanced: language === 'zh' ? '均衡' : 'Balanced', quality: language === 'zh' ? '高质量' : 'Quality' }
  const taskLabel: Record<SmartModelTask, string> = language === 'zh'
    ? { general: '通用', creative: '写作', coding: '编程', reasoning: '推理', vision: '视觉' }
    : { general: 'General', creative: 'Writing', coding: 'Coding', reasoning: 'Reasoning', vision: 'Vision' }
  const scores = (['creative', 'coding', 'reasoning'] as const)
    .map((task) => ({ task, score: rule.task_scores[task] ?? 0 }))
    .sort((a, b) => b.score - a.score)
    .slice(0, 2)
    .map(({ task, score }) => `${taskLabel[task]} ${score}`)
  return [...rule.tiers.map((tier) => tierLabel[tier]), ...scores, ...(rule.capabilities.vision ? [taskLabel.vision] : [])]
}

export function policyModelDisplayName(model: string): string {
  return model
    .replace(/^(?:models\/|openai\/|anthropic\/|google\/)/i, '')
    .split(/[-_./]+/)
    .filter(Boolean)
    .map((part) => {
      const lower = part.toLowerCase()
      if (lower === 'gpt') return 'GPT'
      if (lower === 'claude') return 'Claude'
      if (lower === 'gemini') return 'Gemini'
      if (lower === 'opus') return 'Opus'
      if (lower === 'sonnet') return 'Sonnet'
      return /^\d+$/.test(part) ? part : `${part.charAt(0).toUpperCase()}${part.slice(1)}`
    })
    .join(' ')
}
