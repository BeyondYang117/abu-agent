import type { Settings } from '../../api/tauri'
import { Input, Select, Toggle } from '../../settings/components'
import type { I18n } from '../../settings/i18n'
import { OnboardingFormRow } from '../OnboardingFormRow'
import { OnboardingStepFrame } from '../OnboardingStepFrame'

type WebSearchStepProps = {
  t: I18n
  settings: Settings
  onChange: (settings: Settings) => void
}

export function WebSearchStep({ t, settings, onChange }: WebSearchStepProps) {
  const webSearch = settings.lens?.webSearch ?? {
    enabled: false,
    provider: 'tavily' as const,
    tavilyApiKey: '',
    exaApiKey: '',
    maxResults: 5,
    searchDepth: 'basic' as const,
  }
  const chatWebSearchEnabled = settings.chatTools.nativeTools?.webSearch !== false

  const updateWebSearch = (updates: Partial<NonNullable<Settings['lens']['webSearch']>>) => {
    onChange({
      ...settings,
      lens: {
        ...settings.lens,
        webSearch: {
          ...webSearch,
          ...updates,
        },
      },
    })
  }

  const updateChatWebSearch = (enabled: boolean) => {
    onChange({
      ...settings,
      chatTools: {
        ...settings.chatTools,
        nativeTools: {
          ...settings.chatTools.nativeTools,
          webSearch: enabled,
        },
      },
    })
  }

  const hasApiKey = webSearch.provider === 'exa'
    ? webSearch.exaApiKey.trim() !== ''
    : webSearch.tavilyApiKey.trim() !== ''

  return (
    <OnboardingStepFrame title={t.onboardingWebSearchTitle} subtitle={t.onboardingWebSearchDesc}>
      <div className="onboarding-section">
        <div className="onboarding-section-label">{t.webSearchApiSection}</div>
        <div className="onboarding-card onboarding-card--rows">
          <OnboardingFormRow label={t.lensWebSearchProvider}>
            <Select
              className="w-full max-w-[220px]"
              value={webSearch.provider}
              onChange={(value) => updateWebSearch({ provider: value as 'tavily' | 'exa' })}
              options={[
                { value: 'tavily', label: 'Tavily' },
                { value: 'exa', label: 'Exa' },
              ]}
            />
          </OnboardingFormRow>
          <OnboardingFormRow
            label={t.lensWebSearchApiKey}
            hint={!hasApiKey ? t.onboardingWebSearchKeyRequired : undefined}
            stack
          >
            <Input
              type="password"
              value={webSearch.provider === 'exa' ? webSearch.exaApiKey : webSearch.tavilyApiKey}
              onChange={(value) => {
                if (webSearch.provider === 'exa') {
                  updateWebSearch({ exaApiKey: value })
                } else {
                  updateWebSearch({ tavilyApiKey: value })
                }
              }}
              placeholder={webSearch.provider === 'exa' ? 'exa-...' : 'tvly-...'}
              mono
            />
          </OnboardingFormRow>
        </div>
      </div>

      <div className="onboarding-section">
        <div className="onboarding-section-label">{t.onboardingWebSearchEnableSection}</div>
        <div className="onboarding-card onboarding-card--rows">
          <OnboardingFormRow label={t.webSearchChatSection} hint={t.onboardingWebSearchChatHint}>
            <Toggle
              checked={hasApiKey && chatWebSearchEnabled}
              onChange={(enabled) => {
                if (!hasApiKey) return
                updateChatWebSearch(enabled)
              }}
            />
          </OnboardingFormRow>
          <OnboardingFormRow label={t.webSearchLensSection} hint={t.lensWebSearchHint}>
            <Toggle
              checked={hasApiKey && webSearch.enabled}
              onChange={(enabled) => {
                if (!hasApiKey) return
                updateWebSearch({ enabled })
              }}
            />
          </OnboardingFormRow>
        </div>
      </div>
    </OnboardingStepFrame>
  )
}
