import type { I18n } from '../../settings/i18n'
import type { Lang } from '../../settings/i18n'
import { OnboardingFormRow } from '../OnboardingFormRow'
import { OnboardingStepFrame } from '../OnboardingStepFrame'

type LanguageStepProps = {
  t: I18n
  lang: Lang
  onChange: (lang: Lang) => void
}

export function LanguageStep({ t, lang, onChange }: LanguageStepProps) {
  return (
    <OnboardingStepFrame title={t.onboardingLanguageTitle} subtitle={t.onboardingLanguageDesc}>
      <div className="onboarding-panel">
        <div className="onboarding-panel-section onboarding-panel-section--compact">
          <OnboardingFormRow label={lang === 'zh' ? '显示语言' : 'Display language'} stack>
            <div className="kv-seg">
              <button
                type="button"
                className={lang === 'zh' ? 'active' : ''}
                onClick={() => onChange('zh')}
                data-tauri-drag-region="false"
              >
                中文
              </button>
              <button
                type="button"
                className={lang === 'en' ? 'active' : ''}
                onClick={() => onChange('en')}
                data-tauri-drag-region="false"
              >
                English
              </button>
            </div>
          </OnboardingFormRow>
        </div>
      </div>
    </OnboardingStepFrame>
  )
}
