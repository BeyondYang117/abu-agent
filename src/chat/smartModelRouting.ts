import type { ModelInfo, ModelProvider } from '../api/tauri'
import { resolveModelInfo } from '../data/modelMatching'
import type { ModelRoutingPolicy, ModelRoutingRule } from './modelRoutingPolicy'

export type SmartModelQuality = 'fast' | 'balanced' | 'quality'
export type SmartModelTask = 'vision' | 'coding' | 'creative' | 'reasoning' | 'general'

export type SmartModelRoute = {
  providerId: string
  model: string
  task: SmartModelTask
  policyVersion?: number
  score?: number
  reason?: string
  candidates?: SmartModelCandidateLog[]
  fallbacks?: Array<{ providerId: string; model: string }>
}

export type SmartModelCandidateLog = {
  providerId: string
  model: string
  eligible: boolean
  score: number
  eliminatedReasons: string[]
  breakdown: Record<string, number>
}

export const SMART_MODEL_ENABLED_KEY = 'abu_agent.chat.smartModel.enabled'
export const SMART_MODEL_QUALITY_KEY = 'abu_agent.chat.smartModel.quality'

export function loadSmartModelEnabled(): boolean {
  try {
    const value = window.localStorage.getItem(SMART_MODEL_ENABLED_KEY)
    return value === null ? true : value !== 'false'
  } catch {
    return true
  }
}

export function saveSmartModelEnabled(enabled: boolean): void {
  try {
    window.localStorage.setItem(SMART_MODEL_ENABLED_KEY, String(enabled))
  } catch {
    /* ignore */
  }
}

export function loadSmartModelQuality(): SmartModelQuality {
  try {
    const value = window.localStorage.getItem(SMART_MODEL_QUALITY_KEY)
    if (value === 'fast' || value === 'balanced' || value === 'quality') return value
  } catch {
    /* ignore */
  }
  return 'balanced'
}

export function saveSmartModelQuality(quality: SmartModelQuality): void {
  try {
    window.localStorage.setItem(SMART_MODEL_QUALITY_KEY, quality)
  } catch {
    /* ignore */
  }
}

export function classifySmartModelTask(content: string, hasImage: boolean, policy?: ModelRoutingPolicy | null): SmartModelTask {
  if (hasImage) return 'vision'

  const text = content.toLowerCase()
  for (const mapping of policy?.task_mappings ?? []) {
    if (mapping.keywords.some((keyword) => keyword.trim() && text.includes(keyword.trim().toLowerCase()))) return mapping.task
  }
  if (/(代码|编程|程序|开发|调试|报错|重构|前端|后端|数据库|脚本|函数|组件|api|bug|code|coding|debug|refactor|typescript|javascript|python|rust|java|sql)/i.test(text)) {
    return 'coding'
  }
  if (/(小说|故事|诗|文案|剧本|续写|改写|润色|人物设定|世界观|创意写作|novel|story|poem|copywriting|screenplay|creative writing)/i.test(text)) {
    return 'creative'
  }
  if (/(分析|推理|证明|规划|研究|比较|权衡|策略|数学|逻辑|为什么|analy[sz]e|reason|prove|research|compare|strategy|math|logic)/i.test(text)) {
    return 'reasoning'
  }
  return 'general'
}

function isSpecializedEndpoint(info: ModelInfo): boolean {
  return Boolean(info.capabilities?.embedding || info.capabilities?.imageGeneration)
}

function modelTierScore(model: string, quality: SmartModelQuality): number {
  const name = model.toLowerCase()
  const fast = /(?:mini|nano|haiku|flash|lite|luna|turbo)/.test(name)
  const premium = /(?:opus|pro|max|ultra|thinking|reasoner|\bsol\b)/.test(name)
  if (quality === 'fast') return (fast ? 48 : 0) + (premium ? -18 : 0)
  if (quality === 'quality') return (premium ? 48 : 0) + (fast ? -30 : 0)
  return (fast ? 8 : 0) + (premium ? 12 : 0)
}

function taskScore(task: SmartModelTask, model: string, info: ModelInfo): number {
  const name = model.toLowerCase()
  const capabilities = info.capabilities ?? {}
  switch (task) {
    case 'vision':
      return capabilities.vision ? 80 : -1000
    case 'coding':
      return (/(?:code|coder|codex|codestral|devstral|composer)/.test(name) ? 50 : 0)
        + (capabilities.reasoning ? 18 : 0)
        + (capabilities.functionCalling ? 10 : 0)
    case 'creative':
      return (/(?:claude|opus|sonnet|gpt)/.test(name) ? 26 : 0)
        + (/(?:code|coder|codex)/.test(name) ? -30 : 0)
        + (info.multilingual ? 8 : 0)
    case 'reasoning':
      return (capabilities.reasoning ? 45 : 0)
        + (/(?:thinking|reasoner|opus|\bsol\b)/.test(name) ? 24 : 0)
    default:
      return (capabilities.functionCalling ? 12 : 0) + (capabilities.streaming ? 5 : 0)
  }
}

export function selectSmartModel(input: {
  providers: ModelProvider[]
  content: string
  hasImage: boolean
  quality: SmartModelQuality
  recommended?: string
  current?: { providerId: string; model: string }
  policy?: ModelRoutingPolicy | null
  rolloutKey?: string
}): SmartModelRoute | null {
  if (input.policy) return selectPolicyModel(input as typeof input & { policy: ModelRoutingPolicy })
  const task = classifySmartModelTask(input.content, input.hasImage)
  const candidates = input.providers.flatMap((provider, providerIndex) => {
    if (provider.enabled === false) return []
    const models = provider.enabledModels.length > 0 ? provider.enabledModels : provider.availableModels
    return models.map((model, modelIndex) => {
      const info = resolveModelInfo(model, provider.modelOverrides)
      if (isSpecializedEndpoint(info)) return null
      const currentBonus = input.current?.providerId === provider.id && input.current.model === model ? 3 : 0
      const recommendedBonus = input.recommended === model ? 20 : 0
      return {
        providerId: provider.id,
        model,
        task,
        score: taskScore(task, model, info)
          + modelTierScore(model, input.quality)
          + recommendedBonus
          + currentBonus
          - providerIndex * 0.01
          - modelIndex * 0.0001,
      }
    }).filter((candidate): candidate is SmartModelRoute & { score: number } => candidate !== null)
  })

  const eligible = task === 'vision'
    ? candidates.filter((candidate) => candidate.score > -500)
    : candidates
  eligible.sort((a, b) => b.score - a.score)
  const best = eligible[0]
  return best ? { providerId: best.providerId, model: best.model, task: best.task } : null
}

function stableRolloutEligible(key: string, model: string, percent: number): boolean {
  if (percent >= 100) return true
  if (percent <= 0) return false
  let hash = 0x811c9dc5
  for (const byte of new TextEncoder().encode(`${key}\0${model}`)) {
    hash ^= byte
    hash = Math.imul(hash, 0x01000193)
  }
  return (hash >>> 0) % 100 < percent
}

function taskPolicyScore(rule: ModelRoutingRule, task: SmartModelTask): number {
  return rule.task_scores[task] ?? 0
}

function selectPolicyModel(input: {
  providers: ModelProvider[]
  content: string
  hasImage: boolean
  quality: SmartModelQuality
  recommended?: string
  current?: { providerId: string; model: string }
  policy: ModelRoutingPolicy
  rolloutKey?: string
}): SmartModelRoute | null {
  const task = classifySmartModelTask(input.content, input.hasImage, input.policy)
  const available = new Map<string, Set<string>>()
  for (const provider of input.providers) {
    if (provider.enabled === false) continue
    available.set(provider.id, new Set(provider.enabledModels.length ? provider.enabledModels : provider.availableModels))
  }
  const candidates: SmartModelCandidateLog[] = []
  for (const rule of input.policy.rules) {
    const providerEntries = rule.provider && !available.has('abu-api-relay')
      ? [[rule.provider, available.get(rule.provider)] as const]
      : [...available.entries()].map(([providerId, models]) => [providerId, models] as const)
    for (const [providerId, models] of providerEntries) {
      if (!models?.has(rule.model)) continue
      const eliminatedReasons: string[] = []
      if (!rule.enabled) eliminatedReasons.push('disabled')
      if (!rule.healthy) eliminatedReasons.push(rule.health_reason || 'unhealthy')
      if (!rule.tiers.includes(input.quality)) eliminatedReasons.push('tier_mismatch')
      if ((input.hasImage || task === 'vision') && !rule.capabilities.vision) eliminatedReasons.push('vision_required')
      if (rule.capabilities.embedding || rule.capabilities.image_generation) eliminatedReasons.push('not_chat_model')
      const rolloutModelId = `${rule.provider || providerId}:${rule.model}`
      if (!stableRolloutEligible(input.rolloutKey || 'desktop', rolloutModelId, rule.rollout_percent)) eliminatedReasons.push('outside_rollout')
      const breakdown = {
        task: taskPolicyScore(rule, task),
        tier: 20,
        priority: rule.priority,
        recommended: input.policy.recommended[input.quality] === rule.model || input.recommended === rule.model ? 5 : 0,
        continuity: input.current?.providerId === providerId && input.current.model === rule.model ? 3 : 0,
      }
      candidates.push({
        providerId,
        model: rule.model,
        eligible: eliminatedReasons.length === 0,
        score: Object.values(breakdown).reduce((sum, value) => sum + value, 0),
        eliminatedReasons,
        breakdown,
      })
    }
  }
  candidates.sort((a, b) => Number(b.eligible) - Number(a.eligible) || b.score - a.score)
  const best = candidates.find((candidate) => candidate.eligible)
  if (!best) return null
  const fallbackOrder = input.policy.fallbacks[input.quality] ?? []
  const fallbacks = candidates
    .filter((candidate) => candidate.eligible && candidate !== best)
    .sort((a, b) => {
      const aRule = findPolicyRule(input.policy, a.providerId, a.model)
      const bRule = findPolicyRule(input.policy, b.providerId, b.model)
      const aIndex = fallbackOrder.indexOf(a.model)
      const bIndex = fallbackOrder.indexOf(b.model)
      const normalizedA = aIndex < 0 ? Number.MAX_SAFE_INTEGER : aIndex
      const normalizedB = bIndex < 0 ? Number.MAX_SAFE_INTEGER : bIndex
      return normalizedA - normalizedB
        || (bRule?.fallback_priority ?? 0) - (aRule?.fallback_priority ?? 0)
        || b.score - a.score
    })
    .map(({ providerId, model }) => ({ providerId, model }))
  return {
    providerId: best.providerId,
    model: best.model,
    task,
    score: best.score,
    policyVersion: input.policy.version,
    candidates,
    fallbacks,
    reason: `${best.model} has the highest eligible ${task}/${input.quality} policy score`,
  }
}

function findPolicyRule(policy: ModelRoutingPolicy, providerId: string, model: string): ModelRoutingRule | undefined {
  return policy.rules.find((rule) => rule.model === model && (!rule.provider || providerId === 'abu-api-relay' || rule.provider === providerId))
}
