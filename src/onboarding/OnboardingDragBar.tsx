import type { ReactNode } from 'react'
import { usesNativeTitlebar } from '../chat/platform'

type OnboardingDragBarProps = {
  children?: ReactNode
}

export function OnboardingDragBar({ children }: OnboardingDragBarProps) {
  return (
    <div
      className={`onboarding-drag-bar${usesNativeTitlebar ? ' onboarding-drag-bar--mac' : ' chat-win-titlebar-safe'}`}
      data-tauri-drag-region
    >
      <div className="onboarding-drag-bar-content" data-tauri-drag-region="false">
        {children}
      </div>
      <div className="onboarding-drag-spacer" data-tauri-drag-region />
    </div>
  )
}
