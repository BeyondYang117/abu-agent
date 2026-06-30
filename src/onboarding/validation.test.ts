import { describe, expect, it } from 'vitest'
import type { Settings } from '../api/tauri'
import {
  canCompleteOnboarding,
  providerHasUsableConfig,
  validateProviderStep,
  webSearchConfigured,
} from './validation'

function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    hotkey: 'CommandOrControl+Alt+T',
    theme: 'system',
    themeColor: 'neutral',
    targetLang: 'auto',
    source: 'openai',
    autoPaste: true,
    launchAtStartup: false,
    translatorProviderId: '',
    translatorModel: '',
    chatProviderId: '',
    chatModel: '',
    defaultModels: {
      chat: { providerId: '', model: '' },
      vision: { providerId: '', model: '' },
      titleSummary: { providerId: '', model: '' },
      compression: { providerId: '', model: '' },
      imageGeneration: { providerId: '', model: '' },
    },
    providers: [],
    chatTools: {
      servers: [],
      nativeTools: {},
      approvalPolicy: 'auto',
    },
    retryEnabled: true,
    retryAttempts: 3,
    screenshotTranslation: {
      enabled: true,
      hotkey: 'CommandOrControl+Shift+A',
      textHotkey: 'CommandOrControl+Shift+T',
      providerId: '',
      model: '',
    },
    lens: {
      enabled: true,
      hotkey: 'CommandOrControl+Shift+G',
      providerId: '',
      model: '',
    },
    onboardingStatus: 'pending',
    ...overrides,
  } as Settings
}

const testProvider = {
  id: 'p1',
  name: 'Test',
  apiKeys: ['sk-test'],
  baseUrl: 'https://api.example.com/v1',
  availableModels: ['gpt-4o'],
  enabledModels: ['gpt-4o'],
  supportsTools: true,
  enabled: true,
  apiFormat: 'openai_chat' as const,
}

describe('onboarding validation', () => {
  it('detects usable provider config', () => {
    const settings = baseSettings({
      providers: [testProvider],
    })
    expect(providerHasUsableConfig(settings)).toBe(true)
    expect(validateProviderStep(settings).ok).toBe(false)
  })

  it('requires quick translate, lens, and chat model bindings', () => {
    const settings = baseSettings({
      providers: [testProvider],
      screenshotTranslation: {
        enabled: true,
        hotkey: 'CommandOrControl+Shift+A',
        textHotkey: 'CommandOrControl+Shift+T',
        providerId: 'p1',
        model: 'gpt-4o',
      },
      lens: {
        enabled: true,
        hotkey: 'CommandOrControl+Shift+G',
        providerId: 'p1',
        model: 'gpt-4o',
      },
      defaultModels: {
        chat: { providerId: 'p1', model: 'gpt-4o' },
        vision: { providerId: '', model: '' },
        titleSummary: { providerId: '', model: '' },
        compression: { providerId: '', model: '' },
        imageGeneration: { providerId: '', model: '' },
      },
    })
    expect(canCompleteOnboarding(settings)).toBe(true)
  })

  it('detects configured web search keys', () => {
    const settings = baseSettings({
      lens: {
        enabled: true,
        hotkey: 'CommandOrControl+Shift+G',
        webSearch: {
          enabled: true,
          provider: 'tavily',
          tavilyApiKey: 'tvly-test',
          exaApiKey: '',
          maxResults: 5,
          searchDepth: 'basic',
        },
      },
    })
    expect(webSearchConfigured(settings)).toBe(true)
  })
})
