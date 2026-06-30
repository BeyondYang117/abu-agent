import type { Settings } from '../../api/tauri'
import type { I18n } from '../../settings/i18n'
import type { Lang } from '../../settings/i18n'
import { OnboardingStepFrame } from '../OnboardingStepFrame'
import { ProviderSetupPanel } from '../ProviderSetupPanel'

type ProviderStepProps = {
  t: I18n
  lang: Lang
  settings: Settings
  onChange: (settings: Settings) => void
  showValidationWarning?: boolean
  onBypassValidation?: () => void
  validationBypassed?: boolean
}

export function ProviderStep({
  t,
  lang,
  settings,
  onChange,
  showValidationWarning = false,
  onBypassValidation,
  validationBypassed = false,
}: ProviderStepProps) {
  return (
    <OnboardingStepFrame title={t.onboardingProviderTitle} subtitle={t.onboardingProviderDesc}>
      <ProviderSetupPanel t={t} lang={lang} settings={settings} onChange={onChange} />
      {showValidationWarning ? (
        <div className="onboarding-callout">
          <p className="onboarding-panel-note">{t.onboardingProviderRequired}</p>
          {!validationBypassed && onBypassValidation ? (
            <button
              type="button"
              className="kv-btn ghost"
              onClick={onBypassValidation}
              data-tauri-drag-region="false"
            >
              {t.onboardingProviderContinueAnyway}
            </button>
          ) : null}
        </div>
      ) : null}
    </OnboardingStepFrame>
  )
}
