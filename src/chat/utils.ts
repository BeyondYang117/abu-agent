// Chat 工具函数
export { isTauriRuntime } from '../api/tauri'

/** 用户是否偏好减少动画 */
export const prefersReducedMotion = (): boolean =>
  typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches

/** Empty-chat hero headline: pick one at random for each new empty conversation */
export const CHAT_EMPTY_GREETINGS = [
  'Hey — what are we doing?',
  "Let's get to it.",
  'What should we focus on?',
  'Need a hand with something?',
  "What's the goal?",
  'Where do we start?',
  'What are you trying to solve today?',
  'What should we think through together?',
  "Send it — I've got you.",
  "What's top of mind?",
] as const

export function pickRandomChatEmptyGreeting(): string {
  const index = Math.floor(Math.random() * CHAT_EMPTY_GREETINGS.length)
  return CHAT_EMPTY_GREETINGS[index] ?? CHAT_EMPTY_GREETINGS[0]
}
