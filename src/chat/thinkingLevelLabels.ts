import type { I18n } from '../settings/i18n'

export function thinkingLevelLabel(level: string | null | undefined, t: I18n): string {
  switch (level?.toLowerCase()) {
    case 'off':
      return t.chatThinkingOff
    case 'low':
      return t.chatThinkingLow
    case 'medium':
      return t.chatThinkingMedium
    case 'high':
      return t.chatThinkingHigh
    case 'xhigh':
      return t.chatThinkingXHigh
    case 'max':
      return t.chatThinkingMax
    case 'ultra':
    case 'ultracode':
      return t.chatThinkingUltra
    case undefined:
    case '':
    case 'default':
      return t.chatThinkingAuto
    default:
      return level ?? t.chatThinkingAuto
  }
}

export function thinkingLevelDescription(level: string | null, t: I18n): string {
  switch (level) {
    case 'off':
      return t.chatThinkingOffDescription
    case 'low':
      return t.chatThinkingLowDescription
    case 'medium':
      return t.chatThinkingMediumDescription
    case 'high':
      return t.chatThinkingHighDescription
    case 'xhigh':
      return t.chatThinkingXHighDescription
    case 'max':
      return t.chatThinkingMaxDescription
    case 'ultra':
    case 'ultracode':
      return t.chatThinkingUltraDescription
    default:
      return t.chatThinkingAutoDescription
  }
}
