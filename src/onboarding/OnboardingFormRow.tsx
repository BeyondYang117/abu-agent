import type { ReactNode } from 'react'

type OnboardingFormRowProps = {
  label: string
  hint?: string
  children: ReactNode
  border?: boolean
  stack?: boolean
  extra?: ReactNode
}

export function OnboardingFormRow({
  label,
  hint,
  children,
  border = false,
  stack = false,
  extra,
}: OnboardingFormRowProps) {
  return (
    <div className={`onboarding-form-row${border ? ' onboarding-form-row--border' : ''}${stack ? ' onboarding-form-row--stack' : ''}`}>
      <div className="onboarding-field-copy">
        <div className="onboarding-field-label">{label}</div>
        {hint ? <div className="onboarding-field-hint">{hint}</div> : null}
        {extra}
      </div>
      <div className="onboarding-form-control">{children}</div>
    </div>
  )
}
