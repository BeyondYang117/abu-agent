import { Fragment } from 'react'
import {
  BookOpen,
  ChevronRight,
  Globe,
  Layers,
  MessageSquare,
  ScanLine,
  ScanSearch,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { I18n } from '../../settings/i18n'
import { OnboardingStepFrame } from '../OnboardingStepFrame'

type WelcomeStepProps = {
  t: I18n
}

type FeatureCard = {
  icon: LucideIcon
  title: string
  desc: string
}

export function WelcomeStep({ t }: WelcomeStepProps) {
  const features: FeatureCard[] = [
    { icon: ScanLine, title: t.onboardingWelcomeQuickTranslateTitle, desc: t.onboardingWelcomeQuickTranslateDesc },
    { icon: ScanSearch, title: t.onboardingWelcomeLensTitle, desc: t.onboardingWelcomeLensDesc },
    { icon: MessageSquare, title: t.onboardingWelcomeChatTitle, desc: t.onboardingWelcomeChatDesc },
    { icon: Globe, title: t.onboardingWelcomeWebSearchTitle, desc: t.onboardingWelcomeWebSearchDesc },
    { icon: BookOpen, title: t.onboardingWelcomeKnowledgeTitle, desc: t.onboardingWelcomeKnowledgeDesc },
    { icon: Layers, title: t.onboardingWelcomeMultiProviderTitle, desc: t.onboardingWelcomeMultiProviderDesc },
  ]

  const setupSteps = [
    t.onboardingWelcomeStepProvider,
    t.onboardingWelcomeStepWebSearch,
    t.onboardingWelcomeStepHotkey,
  ]

  return (
    <OnboardingStepFrame title={t.onboardingWelcomeTitle} subtitle={t.onboardingWelcomeSubtitle}>
      <div className="onboarding-section">
        <div className="onboarding-section-label">{t.onboardingWelcomeSectionFeatures}</div>
        <div className="onboarding-feature-grid">
          {features.map(({ icon: Icon, title, desc }) => (
            <article key={title} className="onboarding-feature-card">
              <div className="onboarding-feature-icon">
                <Icon size={18} strokeWidth={1.75} />
              </div>
              <div className="onboarding-feature-copy">
                <h3 className="onboarding-field-label">{title}</h3>
                <p className="onboarding-field-hint">{desc}</p>
              </div>
            </article>
          ))}
        </div>
      </div>

      <div className="onboarding-section">
        <div className="onboarding-section-label">{t.onboardingWelcomeSectionSetup}</div>
        <div className="onboarding-setup-roadmap">
          {setupSteps.map((label, index) => (
            <Fragment key={label}>
              {index > 0 ? <ChevronRight size={14} className="onboarding-setup-arrow" /> : null}
              <div className="onboarding-setup-step">
                <span className="onboarding-setup-step-index">{index + 1}</span>
                <span className="onboarding-setup-step-label">{label}</span>
              </div>
            </Fragment>
          ))}
        </div>
        <p className="onboarding-panel-note">{t.onboardingWelcomeFootnote}</p>
      </div>
    </OnboardingStepFrame>
  )
}
