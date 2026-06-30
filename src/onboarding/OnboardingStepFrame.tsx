import type { ReactNode } from 'react'

type OnboardingStepFrameProps = {
  title: string
  subtitle?: string
  className?: string
  children: ReactNode
}

export function OnboardingStepFrame({ title, subtitle, className, children }: OnboardingStepFrameProps) {
  return (
    <div className={['onboarding-step', className].filter(Boolean).join(' ')}>
      <div className="onboarding-step-head" data-tauri-drag-region="false">
        <h1 className="onboarding-title">{title}</h1>
        {subtitle ? <p className="onboarding-subtitle">{subtitle}</p> : null}
      </div>
      <div className="onboarding-step-content" data-tauri-drag-region="false">
        {children}
      </div>
    </div>
  )
}
