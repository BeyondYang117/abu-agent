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
      <div className="onboarding-panel">
        <div className="onboarding-panel-section">
          <div className="onboarding-panel-label">{t.webSearchApiSection}</div>
          <OnboardingFormRow label={t.lensWebSearchProvider} border>
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
          <OnboardingFormRow label={t.lensWebSearchApiKey} stack>
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

        <div className="onboarding-panel-divider" />

        <div className="onboarding-panel-section">
          <div className="onboarding-panel-label">{t.webSearchChatSection}</div>
          <OnboardingFormRow label={t.webSearchChatToggle} hint={t.webSearchChatHint}>
            <Toggle
              checked={hasApiKey && chatWebSearchEnabled}
              onChange={(enabled) => {
                if (!hasApiKey) return
                updateChatWebSearch(enabled)
              }}
            />
          </OnboardingFormRow>
        </div>

        <div className="onboarding-panel-divider" />

        <div className="onboarding-panel-section">
          <div className="onboarding-panel-label">{t.webSearchLensSection}</div>
          <OnboardingFormRow label={t.enabled} hint={t.lensWebSearchHint}>
            <Toggle
              checked={hasApiKey && webSearch.enabled}
              onChange={(enabled) => {
                if (!hasApiKey) return
                updateWebSearch({ enabled })
              }}
            />
          </OnboardingFormRow>
        </div>

        {!hasApiKey ? (
          <>
            <div className="onboarding-panel-divider" />
            <div className="onboarding-panel-section onboarding-panel-section--compact">
              <p className="onboarding-panel-note">{t.onboardingWebSearchKeyRequired}</p>
            </div>
          </>
        ) : null}
      </div>
    </OnboardingStepFrame>
  )
}
