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

/** Cloud 模式下的引导步骤：跳过 provider 配置，模型列表来自服务端。 */
export const ONBOARDING_STEPS_CLOUD: OnboardingStepId[] = [
  'welcome',
  'login',
  'webSearch',
  'hotkey',
  'done',
]

/**
 * 根据 runtime_mode 返回适用的步骤序列。
 * Cloud 模式跳过 provider 步（无需手填 API Key，模型来自 `abuApi.listModels()`）。
 */
export function getOnboardingSteps(runtimeMode?: string): OnboardingStepId[] {
  const isCloud = runtimeMode?.trim().toLowerCase() === 'cloud'
  return isCloud ? ONBOARDING_STEPS_CLOUD : ONBOARDING_STEPS
}
