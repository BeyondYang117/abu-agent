import { useCallback, useEffect, useMemo, useState } from 'react'
import { ArrowLeft, ArrowRight, Check } from 'lucide-react'
import { type Settings } from '../api/tauri'
import {
  getSettingsCached,
  saveSettingsCached,
  setOnboardingStatusCached,
} from '../api/settingsCache'
import { i18n, type Lang } from '../settings/i18n'
import { usesNativeTitlebar } from '../chat/platform'
import { Button } from '../components/Button'
import { getOnboardingSteps, type OnboardingStepId } from './types'
import { canCompleteOnboarding, validateProviderStep } from './validation'
import { DoneStep } from './steps/DoneStep'
import { HotkeyStep } from './steps/HotkeyStep'
import { LoginStep } from './steps/LoginStep'
import { DEFAULT_ABU_API_BASE_URL } from '../api/abuApi'
import { ProviderStep } from './steps/ProviderStep'
import { WebSearchStep } from './steps/WebSearchStep'
import { WelcomeStep } from './steps/WelcomeStep'
import { completeLogin, useAbuApiAuth } from '../api/abuApiAuth'

type OnboardingShellProps = {
  onComplete: () => void
  onSkip: () => void
  onSettingsChange?: () => void
}

/** 首次运行按系统语言（浏览器/系统 locale）自动选定界面语言：中文 locale → zh，其余 → en。 */
function detectSystemLang(): Lang {
  const raw = (
    (typeof navigator !== 'undefined' && (navigator.language || navigator.languages?.[0])) || ''
  ).toLowerCase()
  return raw.startsWith('zh') ? 'zh' : 'en'
}

export function OnboardingShell({ onComplete, onSkip, onSettingsChange }: OnboardingShellProps) {
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [stepIndex, setStepIndex] = useState(0)
  const [skipConfirmOpen, setSkipConfirmOpen] = useState(false)
  const [providerBypass, setProviderBypass] = useState(false)
  const [loginCompleted, setLoginCompleted] = useState(false)

  const { isAuthenticated } = useAbuApiAuth()
  const loginOnly = new URLSearchParams(window.location.hash.split('?')[1] || '').get('return') === 'chat'
  // Cloud 模式跳过 provider 步，步骤序列动态计算。
  const steps = useMemo(() => getOnboardingSteps(settings?.runtimeMode), [settings?.runtimeMode])
  useEffect(() => {
    const syncRequestedStep = () => {
      const requestedStep = new URLSearchParams(window.location.hash.split('?')[1] || '').get('step')
      if (!requestedStep) return
      const requestedIndex = steps.indexOf(requestedStep as OnboardingStepId)
      if (requestedIndex >= 0) setStepIndex(requestedIndex)
    }
    syncRequestedStep()
    window.addEventListener('hashchange', syncRequestedStep)
    return () => window.removeEventListener('hashchange', syncRequestedStep)
  }, [steps])
  const stepId = steps[stepIndex] ?? 'welcome'
  const lang = (settings?.settingsLanguage || 'zh') as Lang
  const t = i18n[lang]

  const loadSettings = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const loaded = await getSettingsCached()
      // 首次运行按系统语言自动设定界面语言（欢迎页起即本地化）；但若用户此前已选过语言
      // （如重跑引导的老用户），沿用其选择，不要用系统 locale 覆盖。
      setSettings({
        ...loaded,
        settingsLanguage: loaded.settingsLanguage || detectSystemLang(),
      })
    } catch (err) {
      console.error('Failed to load settings for onboarding:', err)
      setLoadError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadSettings()
  }, [loadSettings])

  const updateSettings = useCallback((next: Settings) => {
    setSettings(next)
  }, [])

  const providerValidation = useMemo(
    () => (settings ? validateProviderStep(settings) : { ok: false }),
    [settings],
  )

  const canAdvanceFromProvider = providerValidation.ok || providerBypass

  const canGoNext = useMemo(() => {
    switch (stepId) {
      case 'login':
        return loginCompleted || isAuthenticated
      case 'provider':
        return canAdvanceFromProvider
      default:
        return true
    }
  }, [stepId, loginCompleted, isAuthenticated, canAdvanceFromProvider])

  const persistSettings = useCallback(async (status: 'completed' | 'skipped') => {
    if (!settings) return false
    setSaving(true)
    setSaveError(null)
    try {
      const saved = await saveSettingsCached({
        ...settings,
        onboardingStatus: status,
      })
      setSettings(saved)
      onSettingsChange?.()
      return true
    } catch (err) {
      // 写盘失败必须呈现给用户，避免 Finish/Skip 看起来无响应。
      console.error('Failed to save onboarding settings:', err)
      setSaveError(err instanceof Error ? err.message : String(err))
      return false
    } finally {
      setSaving(false)
    }
  }, [onSettingsChange, settings])

  const handleSkip = useCallback(async () => {
    setSaving(true)
    setSaveError(null)
    try {
      const saved = await setOnboardingStatusCached('skipped')
      setSettings(saved)
      onSettingsChange?.()
      onSkip()
    } catch (err) {
      console.error('Failed to skip onboarding:', err)
      setSaveError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }, [onSettingsChange, onSkip])

  const handleSkipAfterLoadFailure = useCallback(async () => {
    setSaving(true)
    setLoadError(null)
    try {
      const saved = await setOnboardingStatusCached('skipped')
      setSettings(saved)
      onSettingsChange?.()
      onSkip()
    } catch (err) {
      console.error('Failed to skip onboarding after load error:', err)
      setLoadError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }, [onSettingsChange, onSkip])

  const handleFinish = useCallback(async () => {
    if (!settings) return
    // 正常完成需通过供应商校验；若用户显式「继续（跳过校验）」，则以 skipped 状态完成——
    // 供应商未验证，标 skipped 比 completed 诚实，且同样不会再次弹引导。避免 bypass 后
    // Finish 永久禁用、只能走 Skip 的死路。
    if (canCompleteOnboarding(settings)) {
      const ok = await persistSettings('completed')
      if (ok) onComplete()
    } else if (providerBypass) {
      const ok = await persistSettings('skipped')
      if (ok) onComplete()
    }
  }, [onComplete, persistSettings, providerBypass, settings])

  const goNext = () => {
    if (stepIndex >= steps.length - 1) return
    setStepIndex((index) => Math.min(index + 1, steps.length - 1))
  }

  const goBack = () => {
    setStepIndex((index) => Math.max(index - 1, 0))
  }

  if (loading) {
    return (
      <div className="onboarding-shell onboarding-shell--loading settings-embedded kv">
        <div className="h-5 w-5 animate-spin rounded-full border-2 border-neutral-300 border-t-neutral-800 dark:border-neutral-700 dark:border-t-neutral-200" />
      </div>
    )
  }

  if (!settings) {
    const errorT = i18n.zh
    return (
      <div className="onboarding-shell onboarding-shell--loading settings-embedded kv">
        <div className="onboarding-error-panel">
          <h2 className="onboarding-title">{errorT.onboardingLoadErrorTitle}</h2>
          <p className="onboarding-subtitle">{errorT.onboardingLoadErrorDesc}</p>
          {loadError ? <p className="onboarding-panel-note">{loadError}</p> : null}
          <div className="onboarding-error-actions">
            <Button
              variant="primary"
              onClick={() => void loadSettings()}
              disabled={saving}
              data-tauri-drag-region="false"
            >
              {errorT.onboardingRetry}
            </Button>
            <Button
              variant="ghost"
              onClick={() => void handleSkipAfterLoadFailure()}
              disabled={saving}
              data-tauri-drag-region="false"
            >
              {errorT.onboardingSkip}
            </Button>
          </div>
        </div>
      </div>
    )
  }

  const stepLabels: Record<OnboardingStepId, string> = {
    welcome: t.onboardingStepWelcome,
    login: t.onboardingStepLogin || '登录',
    provider: t.onboardingWelcomeStepProvider,
    webSearch: t.onboardingWelcomeStepWebSearch,
    hotkey: t.onboardingWelcomeStepHotkey,
    done: t.onboardingStepDone,
  }

  const primaryLabel = stepId === 'welcome'
    ? t.onboardingStart
    : stepId === 'done'
      ? t.onboardingFinish
      : t.onboardingNext

  const handlePrimary = () => {
    if (stepId === 'done') {
      void handleFinish()
      return
    }
    goNext()
  }

  const handleLoginSuccess = async (sessionToken: string) => {
    try {
      await completeLogin(sessionToken)
      setLoginCompleted(true)
      const returnToChat = new URLSearchParams(window.location.hash.split('?')[1] || '').get('return') === 'chat'
      if (returnToChat) {
        onComplete()
        return
      }
      // 重新加载 settings 以获取更新后的 runtime_mode
      const reloaded = await getSettingsCached()
      setSettings(reloaded)
      // 自动进入下一步
      goNext()
    } catch (err) {
      console.error('Failed to complete login:', err)
      setSaveError(err instanceof Error ? err.message : String(err))
    }
  }

  if (loginOnly) {
    return (
      <div className="onboarding-shell onboarding-login-only settings-embedded kv">
        <div className="onboarding-main">
          <div className="onboarding-body kv-scroll" data-tauri-drag-region="false">
            <div className="onboarding-login-only-inner">
              <div className="onboarding-login-only-brand" aria-hidden="true">
                <img src="/logo-mark.png" alt="" draggable={false} />
                <span>ABU Agent</span>
              </div>
              <div className="onboarding-login-only-card">
                <LoginStep
                  t={t}
                  abuApiBaseUrl={settings.abu_api_base_url || DEFAULT_ABU_API_BASE_URL}
                  onLoginSuccess={handleLoginSuccess}
                />
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="onboarding-shell settings-embedded kv">
      <aside
        className={`onboarding-side${usesNativeTitlebar ? ' onboarding-side--mac' : ''}`}
        data-tauri-drag-region
      >
        <div className="onboarding-side-brand" data-tauri-drag-region>
          <img src="/logo-mark.png" alt="" className="onboarding-side-logo" draggable={false} />
          <span className="onboarding-side-brand-name">ABU Agent</span>
        </div>
        <nav className="onboarding-side-steps">
          {steps.map((step, index) => {
            const done = index < stepIndex
            const active = index === stepIndex
            return (
              <button
                key={step}
                type="button"
                className={`onboarding-side-step${active ? ' active' : ''}${done ? ' done' : ''}`}
                data-clickable={done ? 'true' : 'false'}
                disabled={!done}
                onClick={() => {
                  if (done) setStepIndex(index)
                }}
                data-tauri-drag-region="false"
              >
                <span className="onboarding-side-step-bullet">
                  {done ? <Check size={11} strokeWidth={3} /> : index + 1}
                </span>
                <span className="onboarding-side-step-label">{stepLabels[step]}</span>
              </button>
            )
          })}
        </nav>
      </aside>

      <div className="onboarding-main">
        <div className="onboarding-topbar" data-tauri-drag-region>
          {/* Cloud 模式下隐藏"跳过引导"：用户已通过 ABU API 授权登录，应完成完整引导流程 */}
          {settings.runtimeMode?.trim().toLowerCase() !== 'cloud' && (
            <Button
              variant="ghost"
              onClick={() => setSkipConfirmOpen(true)}
              data-tauri-drag-region="false"
            >
              {t.onboardingSkip}
            </Button>
          )}
        </div>

        <div className="onboarding-body kv-scroll" data-tauri-drag-region="false">
          {stepId === 'welcome' ? <WelcomeStep t={t} /> : null}
          {stepId === 'login' ? (
            <LoginStep
              t={t}
              abuApiBaseUrl={settings?.abu_api_base_url || DEFAULT_ABU_API_BASE_URL}
              onLoginSuccess={handleLoginSuccess}
            />
          ) : null}
          {stepId === 'provider' ? (
            <ProviderStep
              t={t}
              lang={lang}
              settings={settings}
              onChange={updateSettings}
              showValidationWarning={!providerValidation.ok}
              validationBypassed={providerBypass}
              onBypassValidation={() => setProviderBypass(true)}
            />
          ) : null}
          {stepId === 'webSearch' ? (
            <WebSearchStep t={t} settings={settings} onChange={updateSettings} />
          ) : null}
          {stepId === 'hotkey' ? (
            <HotkeyStep t={t} settings={settings} onChange={updateSettings} />
          ) : null}
          {stepId === 'done' ? <DoneStep t={t} settings={settings} /> : null}
        </div>

        <div className="onboarding-footer" data-tauri-drag-region="false">
          {saveError ? (
            <div className="onboarding-footer-error" role="alert">
              <span>{t.onboardingSaveError}</span>
              <span className="onboarding-footer-error-detail">{saveError}</span>
            </div>
          ) : null}
          <div className="onboarding-footer-inner">
            {stepIndex > 0 ? (
              <Button
                variant="ghost"
                onClick={goBack}
                disabled={saving}
                data-tauri-drag-region="false"
              >
                <ArrowLeft size={14} />
                {t.onboardingBack}
              </Button>
            ) : null}
            <div className="onboarding-footer-spacer" />
            <div className="onboarding-footer-actions">
              {stepId === 'login' ? (
                <Button
                  variant="ghost"
                  onClick={goNext}
                  disabled={saving}
                  data-tauri-drag-region="false"
                >
                  {t.onboardingLoginSkipStep}
                </Button>
              ) : null}
              {stepId === 'webSearch' ? (
                <Button
                  variant="ghost"
                  onClick={goNext}
                  disabled={saving}
                  data-tauri-drag-region="false"
                >
                  {t.onboardingWebSearchSkipStep}
                </Button>
              ) : null}
              <Button
                variant="primary"
                onClick={handlePrimary}
                disabled={saving || (stepId !== 'done' && !canGoNext) || (stepId === 'done' && !canCompleteOnboarding(settings) && !providerBypass)}
                data-tauri-drag-region="false"
              >
                {primaryLabel}
                {stepId !== 'done' ? <ArrowRight size={14} /> : null}
              </Button>
            </div>
          </div>
        </div>
      </div>

      {skipConfirmOpen ? (
        <div
          className="kv-modal-backdrop kv-modal-backdrop--portal"
          data-tauri-drag-region="false"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setSkipConfirmOpen(false)
          }}
        >
          <div
            className="kv-modal"
            role="dialog"
            aria-modal="true"
            data-tauri-drag-region="false"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <h3 className="kv-modal-title">{t.onboardingSkipConfirmTitle}</h3>
            <p className="kv-row-desc">{t.onboardingSkipConfirmDesc}</p>
            <div className="flex justify-end gap-2 pt-4">
              <Button
                variant="ghost"
                onClick={() => setSkipConfirmOpen(false)}
                data-tauri-drag-region="false"
              >
                {t.cancel}
              </Button>
              <Button
                variant="primary"
                onClick={() => {
                  setSkipConfirmOpen(false)
                  void handleSkip()
                }}
                disabled={saving}
                data-tauri-drag-region="false"
              >
                {t.onboardingSkipConfirm}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  )
}
