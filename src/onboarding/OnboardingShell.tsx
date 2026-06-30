import { useCallback, useEffect, useMemo, useState } from 'react'
import { ArrowLeft, ArrowRight } from 'lucide-react'
import { api, type Settings } from '../api/tauri'
import { i18n, type Lang } from '../settings/i18n'
import { OnboardingDragBar } from './OnboardingDragBar'
import { ONBOARDING_STEPS } from './types'
import { canCompleteOnboarding, validateProviderStep } from './validation'
import { DoneStep } from './steps/DoneStep'
import { HotkeyStep } from './steps/HotkeyStep'
import { LanguageStep } from './steps/LanguageStep'
import { ProviderStep } from './steps/ProviderStep'
import { WebSearchStep } from './steps/WebSearchStep'
import { WelcomeStep } from './steps/WelcomeStep'

type OnboardingShellProps = {
  onComplete: () => void
  onSkip: () => void
  onSettingsChange?: () => void
}

export function OnboardingShell({ onComplete, onSkip, onSettingsChange }: OnboardingShellProps) {
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [stepIndex, setStepIndex] = useState(0)
  const [skipConfirmOpen, setSkipConfirmOpen] = useState(false)
  const [providerBypass, setProviderBypass] = useState(false)

  const stepId = ONBOARDING_STEPS[stepIndex] ?? 'welcome'
  const lang = (settings?.settingsLanguage || 'zh') as Lang
  const t = i18n[lang]

  const loadSettings = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const loaded = await api.getSettings()
      setSettings(loaded)
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

  const updateLanguage = useCallback((nextLang: Lang) => {
    setSettings((current) => current ? { ...current, settingsLanguage: nextLang } : current)
  }, [])

  const providerValidation = useMemo(
    () => (settings ? validateProviderStep(settings) : { ok: false }),
    [settings],
  )

  const canAdvanceFromProvider = providerValidation.ok || providerBypass

  const canGoNext = useMemo(() => {
    switch (stepId) {
      case 'provider':
        return canAdvanceFromProvider
      default:
        return true
    }
  }, [canAdvanceFromProvider, stepId])

  const persistSettings = useCallback(async (status: 'completed' | 'skipped') => {
    if (!settings) return false
    setSaving(true)
    try {
      const saved = await api.saveSettings({
        ...settings,
        onboardingStatus: status,
      })
      setSettings(saved)
      onSettingsChange?.()
      return true
    } catch (err) {
      console.error('Failed to save onboarding settings:', err)
      return false
    } finally {
      setSaving(false)
    }
  }, [onSettingsChange, settings])

  const handleSkip = useCallback(async () => {
    const ok = await persistSettings('skipped')
    if (ok) onSkip()
  }, [onSkip, persistSettings])

  const handleSkipAfterLoadFailure = useCallback(async () => {
    setSaving(true)
    try {
      const loaded = await api.getSettings()
      await api.saveSettings({ ...loaded, onboardingStatus: 'skipped' })
      onSettingsChange?.()
    } catch (err) {
      console.error('Failed to skip onboarding after load error:', err)
    } finally {
      setSaving(false)
    }
    onSkip()
  }, [onSettingsChange, onSkip])

  const handleFinish = useCallback(async () => {
    if (!settings || !canCompleteOnboarding(settings)) return
    const ok = await persistSettings('completed')
    if (ok) onComplete()
  }, [onComplete, persistSettings, settings])

  const goNext = () => {
    if (stepIndex >= ONBOARDING_STEPS.length - 1) return
    setStepIndex((index) => Math.min(index + 1, ONBOARDING_STEPS.length - 1))
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
            <button
              type="button"
              className="kv-btn primary"
              onClick={() => void loadSettings()}
              disabled={saving}
              data-tauri-drag-region="false"
            >
              {errorT.onboardingRetry}
            </button>
            <button
              type="button"
              className="kv-btn ghost"
              onClick={() => void handleSkipAfterLoadFailure()}
              disabled={saving}
              data-tauri-drag-region="false"
            >
              {errorT.onboardingSkip}
            </button>
          </div>
        </div>
      </div>
    )
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

  return (
    <div className="onboarding-shell settings-embedded kv">
      <OnboardingDragBar>
        <div className="onboarding-brand">Kivio</div>
        <div className="onboarding-progress">
          {ONBOARDING_STEPS.map((step, index) => (
            <span
              key={step}
              className={`onboarding-progress-dot${index === stepIndex ? ' active' : ''}${index < stepIndex ? ' done' : ''}`}
              title={step}
            />
          ))}
        </div>
        <button
          type="button"
          className="kv-btn ghost onboarding-skip-btn"
          onClick={() => setSkipConfirmOpen(true)}
          data-tauri-drag-region="false"
        >
          {t.onboardingSkip}
        </button>
      </OnboardingDragBar>

      <div className="onboarding-body-layout">
        <div className="onboarding-drag-rail" data-tauri-drag-region aria-hidden="true" />
        <div className="onboarding-body kv-scroll" data-tauri-drag-region="false">
          {stepId === 'welcome' ? <WelcomeStep t={t} /> : null}
          {stepId === 'language' ? (
            <LanguageStep t={t} lang={lang} onChange={updateLanguage} />
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
        <div className="onboarding-drag-rail" data-tauri-drag-region aria-hidden="true" />
      </div>

      <div className="onboarding-footer" data-tauri-drag-region="false">
        <button
          type="button"
          className="kv-btn ghost"
          onClick={goBack}
          disabled={stepIndex === 0 || saving}
          data-tauri-drag-region="false"
        >
          <ArrowLeft size={14} />
          {t.onboardingBack}
        </button>
        <div className="onboarding-drag-spacer" data-tauri-drag-region />
        <div className="onboarding-footer-actions" data-tauri-drag-region="false">
          {stepId === 'webSearch' ? (
            <button
              type="button"
              className="kv-btn ghost"
              onClick={goNext}
              disabled={saving}
              data-tauri-drag-region="false"
            >
              {t.onboardingWebSearchSkipStep}
            </button>
          ) : null}
          <button
            type="button"
            className="kv-btn primary"
            onClick={handlePrimary}
            disabled={saving || (stepId !== 'done' && !canGoNext) || (stepId === 'done' && !canCompleteOnboarding(settings))}
            data-tauri-drag-region="false"
          >
            {primaryLabel}
            {stepId !== 'done' ? <ArrowRight size={14} /> : null}
          </button>
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
              <button
                type="button"
                className="kv-btn ghost"
                onClick={() => setSkipConfirmOpen(false)}
                data-tauri-drag-region="false"
              >
                {t.cancel}
              </button>
              <button
                type="button"
                className="kv-btn primary"
                onClick={() => {
                  setSkipConfirmOpen(false)
                  void handleSkip()
                }}
                disabled={saving}
                data-tauri-drag-region="false"
              >
                {t.onboardingSkipConfirm}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  )
}
