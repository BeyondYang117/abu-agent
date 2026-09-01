export type OnboardingStepId =
  | 'welcome'
  | 'login'
  | 'provider'
  | 'webSearch'
  | 'hotkey'
  | 'done'

// 不做语言选择步：首次运行按系统语言自动设定（见 OnboardingShell 的 detectSystemLang），
// 之后可在「设置 → 基础」里随时改。
// login 步骤：使用 ABU API 账户登录（Device Code Flow 或密码登录）
export const ONBOARDING_STEPS: OnboardingStepId[] = [
  'welcome',
  'login',
  'provider',
  'webSearch',
  'hotkey',
  'done',
]
