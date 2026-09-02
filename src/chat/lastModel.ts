/**
 * 记住用户在顶栏最后一次选的聊天模型，作为新会话/空会话草稿。
 * 以聊天界面的选择为准，不再单独设「Chat 默认模型」。仅前端偏好，存 localStorage。
 */

export const LAST_MODEL_KEY = 'kivio.chat.lastModel'

export type ChatModelBinding = {
  providerId: string
  model: string
}

export function loadLastModel(): ChatModelBinding | null {
  try {
    if (typeof window === 'undefined') return null
    const raw = window.localStorage.getItem(LAST_MODEL_KEY)
    if (!raw) return null
    const value = JSON.parse(raw) as Partial<ChatModelBinding>
    if (
      value
      && typeof value.providerId === 'string'
      && typeof value.model === 'string'
      && value.providerId
    ) {
      return { providerId: value.providerId, model: value.model }
    }
  } catch {
    /* ignore */
  }
  return null
}

export function saveLastModel(providerId: string, model: string): void {
  try {
    if (typeof window === 'undefined' || !providerId) return
    window.localStorage.setItem(LAST_MODEL_KEY, JSON.stringify({ providerId, model }))
  } catch {
    /* ignore */
  }
}

type ChatModelProvider = {
  id: string
  enabledModels?: string[]
  availableModels?: string[]
}

function providerForModel(
  providers: ChatModelProvider[],
  binding: ChatModelBinding,
): ChatModelProvider | undefined {
  if (!binding.providerId) return undefined
  const provider = providers.find((candidate) => candidate.id === binding.providerId)
  if (!provider) return undefined
  const models = provider.enabledModels?.length
    ? provider.enabledModels
    : provider.availableModels
  if (models?.length && !models.includes(binding.model)) return undefined
  return provider
}

function firstAvailableModel(providers: ChatModelProvider[]): ChatModelBinding | undefined {
  for (const provider of providers) {
    const models = provider.enabledModels?.length ? provider.enabledModels : provider.availableModels
    const model = models?.find((candidate) => candidate.trim())
    if (model) return { providerId: provider.id, model }
  }
  return undefined
}

/**
 * 聊天草稿 / 设置页「当前模型」共用同一条回落：上次选择 → 已写入的 last-used →
 * 旧字段 chatProviderId → Lens → 翻译。
 */
export function resolvePreferredChatModel(input: {
  providers: ChatModelProvider[]
  last: ChatModelBinding | null
  storedChat: ChatModelBinding
  legacyChat: ChatModelBinding
  lens: ChatModelBinding
  translator: ChatModelBinding
}): ChatModelBinding {
  if (input.last && providerForModel(input.providers, input.last)) {
    return input.last
  }
  if (providerForModel(input.providers, input.storedChat)) {
    return input.storedChat
  }
  if (providerForModel(input.providers, input.legacyChat)) {
    return input.legacyChat
  }
  // A configured model may have been removed from the provider catalog. Prefer
  // the first currently available model over stale Lens/translator fallbacks.
  const first = firstAvailableModel(input.providers)
  if (first) return first
  if (providerForModel(input.providers, input.lens)) {
    return input.lens
  }
  return input.translator
}
