import type { Settings } from '../api/tauri'

export function providerHasUsableConfig(settings: Settings): boolean {
  return settings.providers.some((provider) =>
    provider.enabled !== false
    && provider.apiKeys.some((key) => key.trim() !== '')
    && provider.enabledModels.length > 0,
  )
}

export function validateProviderStep(settings: Settings): { ok: boolean; reason?: string } {
  if (settings.providers.length === 0) {
    return { ok: false, reason: 'no_provider' }
  }
  const provider = settings.providers.find((item) =>
    item.enabled !== false
    && item.apiKeys.some((key) => key.trim() !== ''),
  )
  if (!provider) {
    return { ok: false, reason: 'missing_api_key' }
  }
  if (provider.enabledModels.length === 0) {
    return { ok: false, reason: 'no_enabled_models' }
  }
  const quickProviderId = settings.screenshotTranslation?.providerId?.trim() ?? ''
  const quickModel = settings.screenshotTranslation?.model?.trim() ?? ''
  if (!quickProviderId || !quickModel) {
    return { ok: false, reason: 'missing_quick_translate_model' }
  }
  const lensProviderId = settings.lens?.providerId?.trim() ?? ''
  const lensModel = settings.lens?.model?.trim() ?? ''
  if (!lensProviderId || !lensModel) {
    return { ok: false, reason: 'missing_lens_model' }
  }
  const chatProviderId = settings.defaultModels.chat.providerId.trim()
  const chatModel = settings.defaultModels.chat.model.trim()
  if (!chatProviderId || !chatModel) {
    return { ok: false, reason: 'missing_chat_model' }
  }
  return { ok: true }
}

export function canCompleteOnboarding(settings: Settings): boolean {
  return validateProviderStep(settings).ok
}

export function webSearchConfigured(settings: Settings): boolean {
  const webSearch = settings.lens?.webSearch
  if (!webSearch) return false
  if (webSearch.provider === 'exa') {
    return webSearch.exaApiKey.trim() !== ''
  }
  return webSearch.tavilyApiKey.trim() !== ''
}
