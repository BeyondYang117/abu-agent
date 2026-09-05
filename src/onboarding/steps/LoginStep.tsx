import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ExternalLink, Loader2, Copy, Check } from 'lucide-react'
import type { I18n } from '../../settings/i18n'
import { Button } from '../../components/Button'
import { AbuApiClient } from '../../api/abuApi'
import { api, isTauriRuntime } from '../../api/tauri'

type LoginStepProps = {
  t: I18n
  abuApiBaseUrl: string
  onLoginSuccess: (sessionToken: string) => void
}

type LoginMode = 'device' | 'password'

type DeviceFlowState = {
  deviceCode: string
  userCode: string
  verificationUri: string
  expiresAt: number
}

const DEVICE_REQUEST_TIMEOUT_MS = 20_000

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timeoutId = window.setTimeout(() => reject(new Error(message)), timeoutMs)
    promise.then(
      (value) => {
        window.clearTimeout(timeoutId)
        resolve(value)
      },
      (error) => {
        window.clearTimeout(timeoutId)
        reject(error)
      },
    )
  })
}

function buildDeviceAuthorizationUrl(
  baseUrl: string,
  verificationUri: string,
  userCode: string,
): string {
  return `${baseUrl}${verificationUri}?user_code=${userCode}`
}

export function LoginStep({ t, abuApiBaseUrl, onLoginSuccess }: LoginStepProps) {
  const [mode, setMode] = useState<LoginMode>('device')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Device Code Flow state
  const [deviceFlow, setDeviceFlow] = useState<DeviceFlowState | null>(null)
  const [polling, setPolling] = useState(false)

  // Password login state
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')

  // Copy state
  const [copied, setCopied] = useState(false)
  const pollingGenerationRef = useRef(0)
  const pollingTimerRef = useRef<number | null>(null)

  // 创建临时客户端（登录前不需要 session token）
  const client = useMemo(() => new AbuApiClient(abuApiBaseUrl), [abuApiBaseUrl])

  // 复制验证码
  const copyCode = useCallback(async () => {
    if (!deviceFlow) return
    try {
      await navigator.clipboard.writeText(deviceFlow.userCode)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch (err) {
      console.error('Failed to copy:', err)
    }
  }, [deviceFlow])

  const stopPolling = useCallback(() => {
    pollingGenerationRef.current += 1
    if (pollingTimerRef.current !== null) {
      window.clearTimeout(pollingTimerRef.current)
      pollingTimerRef.current = null
    }
    setPolling(false)
  }, [])

  useEffect(() => () => {
    pollingGenerationRef.current += 1
    if (pollingTimerRef.current !== null) window.clearTimeout(pollingTimerRef.current)
  }, [])

  // 串行轮询兑换 token，避免慢网络下 setInterval 叠加多个未完成请求。
  const startPolling = useCallback(
    (deviceCode: string, intervalSeconds: number) => {
      stopPolling()
      setPolling(true)
      const generation = pollingGenerationRef.current
      const expiresAt = Date.now() + 10 * 60 * 1000

      const poll = async () => {
        if (pollingGenerationRef.current !== generation) return
        try {
          const request = isTauriRuntime()
            ? api.abuApiExchangeDeviceAuthorization(abuApiBaseUrl, deviceCode)
            : client.exchangeDeviceAuthorization(deviceCode)
          const response = await withTimeout(
            request,
            DEVICE_REQUEST_TIMEOUT_MS,
            t.onboardingLoginTimeout || '网络请求超时，请检查网络后重试',
          )

          if (pollingGenerationRef.current !== generation) return

          if (response.status === 'consumed' && response.session_token) {
            stopPolling()
            onLoginSuccess(response.session_token)
            return
          } else if (response.status === 'denied') {
            stopPolling()
            setError(t.onboardingLoginDenied || '用户拒绝了授权请求')
            return
          } else if (response.status === 'expired') {
            stopPolling()
            setError(t.onboardingLoginExpired || '授权请求已过期，请重新开始')
            setDeviceFlow(null)
            return
          }
          // pending 或 approved 时继续轮询
        } catch (err) {
          const timeoutMessage = t.onboardingLoginTimeout || '网络请求超时，请检查网络后重试'
          if (err instanceof Error && err.message === timeoutMessage) {
            // Tauri invoke 无法取消底层 reqwest 请求；超时后停止本轮，避免慢请求
            // 与下一轮叠加，并把控制权交还给用户重试。
            stopPolling()
            setError(timeoutMessage)
            setDeviceFlow(null)
            return
          }
          // 短暂网络错误时继续轮询。
          console.warn('Polling error:', err)
        }

        if (pollingGenerationRef.current !== generation) return
        if (Date.now() >= expiresAt) {
          stopPolling()
          setError(t.onboardingLoginTimeout || '授权超时，请重新开始')
          setDeviceFlow(null)
          return
        }

        pollingTimerRef.current = window.setTimeout(poll, Math.max(1, intervalSeconds) * 1000)
      }

      pollingTimerRef.current = window.setTimeout(poll, Math.max(1, intervalSeconds) * 1000)
    },
    [abuApiBaseUrl, client, onLoginSuccess, stopPolling, t],
  )

  // 开始 Device Code Flow
  const startDeviceFlow = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      // 1. 获取设备名称（用于显示）
      const deviceName = await withTimeout(
        getDeviceName(),
        5_000,
        t.onboardingLoginTimeout || '读取设备信息超时，请重试',
      )

      // 2. 请求 device code
      const request = isTauriRuntime()
        ? api.abuApiCreateDeviceAuthorization(abuApiBaseUrl, deviceName)
        : client.createDeviceAuthorization(deviceName)
      const response = await withTimeout(
        request,
        DEVICE_REQUEST_TIMEOUT_MS,
        t.onboardingLoginTimeout || '连接 ABU API 超时，请检查网络后重试',
      )
      setDeviceFlow({
        deviceCode: response.device_code,
        userCode: response.user_code,
        verificationUri: response.verification_uri,
        expiresAt: response.expires_at,
      })

      // 3. 打开浏览器到验证页面
      const verificationUrl = buildDeviceAuthorizationUrl(
        abuApiBaseUrl,
        response.verification_uri,
        response.user_code,
      )
      // 打开浏览器不是轮询授权的前置条件。Windows 默认浏览器调用异常时，
      // 用户仍可复制验证码或点击“重新打开浏览器”，页面不能卡在 loading。
      void api.openExternal(verificationUrl).catch((err) => {
        const detail = err instanceof Error ? err.message : String(err)
        setError(`授权请求已创建，但无法打开系统浏览器：${detail}`)
      })

      // 4. 开始轮询，不等待系统浏览器调用返回
      startPolling(response.device_code, response.interval)
    } catch (err) {
      console.error('Failed to start device flow:', err)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [abuApiBaseUrl, client, startPolling, t])

  // 密码登录
  const handlePasswordLogin = useCallback(async () => {
    if (!username.trim() || !password) {
      setError(t.onboardingLoginEmptyCredentials || '请输入用户名和密码')
      return
    }

    setLoading(true)
    setError(null)
    try {
      const response = await client.loginWithPassword(username.trim(), password)
      onLoginSuccess(response.session_token)
    } catch (err) {
      console.error('Password login failed:', err)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [client, username, password, onLoginSuccess, t])

  // 取消 Device Flow
  const cancelDeviceFlow = useCallback(() => {
    stopPolling()
    setDeviceFlow(null)
    setError(null)
  }, [stopPolling])

  return (
    <div className="onboarding-step">
      <h2 className="onboarding-title">{t.onboardingLoginTitle || '登录 ABU API 账户'}</h2>
      <p className="onboarding-subtitle">
        {t.onboardingLoginDesc ||
          '使用 ABU API 账户，您可以获得统一的配额管理、使用统计和多设备同步'}
      </p>

      {error ? (
        <div className="onboarding-error-banner" role="alert">
          {error}
        </div>
      ) : null}

      {mode === 'device' ? (
        <div className="onboarding-login-device">
          {!deviceFlow ? (
            <>
              <p className="onboarding-panel-note">
                {t.onboardingLoginDeviceDesc ||
                  '点击下方按钮将在浏览器中打开授权页面，登录后即可自动完成桌面端授权'}
              </p>
              <Button
                variant="primary"
                onClick={startDeviceFlow}
                disabled={loading}
                data-tauri-drag-region="false"
              >
                {loading ? (
                  <>
                    <Loader2 size={14} className="animate-spin" />
                    {t.onboardingLoginStarting || '正在准备...'}
                  </>
                ) : (
                  <>
                    <ExternalLink size={14} />
                    {t.onboardingLoginWithBrowser || '在浏览器中登录'}
                  </>
                )}
              </Button>
            </>
          ) : (
            <div className="onboarding-login-waiting">
              <div className="onboarding-login-status">
                {polling ? (
                  <Loader2 size={20} className="animate-spin text-blue-600 dark:text-blue-400" />
                ) : null}
                <p className="onboarding-login-status-text">
                  {t.onboardingLoginWaitingForApproval || '等待浏览器授权...'}
                </p>
              </div>

              <div className="onboarding-login-code-block">
                <p className="text-sm text-neutral-600 dark:text-neutral-400">
                  {t.onboardingLoginCodeLabel || '验证码：'}
                </p>
                <div className="onboarding-login-code-container">
                  <span className="onboarding-login-code">{deviceFlow.userCode}</span>
                  <button
                    type="button"
                    onClick={copyCode}
                    className="onboarding-login-copy-btn"
                    data-tauri-drag-region="false"
                    title={copied ? (t.lensCopied || '已复制') : (t.lensCopy || '复制')}
                  >
                    {copied ? (
                      <>
                        <Check size={14} className="text-green-600 dark:text-green-400" />
                        <span>{t.lensCopied || '已复制'}</span>
                      </>
                    ) : (
                      <>
                        <Copy size={14} />
                        <span>{t.lensCopy || '复制'}</span>
                      </>
                    )}
                  </button>
                </div>
                <p className="text-xs text-neutral-500 dark:text-neutral-500">
                  {t.onboardingLoginCodeHint ||
                    '如果浏览器未自动填入验证码，请手动输入上方代码'}
                </p>
              </div>

              <div className="flex gap-2 justify-center">
                <Button
                  variant="ghost"
                  onClick={cancelDeviceFlow}
                  disabled={loading}
                  data-tauri-drag-region="false"
                >
                  {t.cancel || '取消'}
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => {
                    const url = buildDeviceAuthorizationUrl(
                      abuApiBaseUrl,
                      deviceFlow.verificationUri,
                      deviceFlow.userCode,
                    )
                    void api.openExternal(url).catch((err) => {
                      setError(err instanceof Error ? err.message : String(err))
                    })
                  }}
                  data-tauri-drag-region="false"
                >
                  <ExternalLink size={14} />
                  {t.onboardingLoginReopenBrowser || '重新打开浏览器'}
                </Button>
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="onboarding-login-password">
          <div className="onboarding-field">
            <label htmlFor="username" className="onboarding-field-label">
              {t.onboardingLoginUsername || '用户名'}
            </label>
            <input
              id="username"
              type="text"
              className="kv-input"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              disabled={loading}
              autoComplete="username"
              placeholder={t.onboardingLoginUsernamePlaceholder || '请输入用户名或邮箱'}
            />
          </div>

          <div className="onboarding-field">
            <label htmlFor="password" className="onboarding-field-label">
              {t.onboardingLoginPassword || '密码'}
            </label>
            <input
              id="password"
              type="password"
              className="kv-input"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handlePasswordLogin()
              }}
              disabled={loading}
              autoComplete="current-password"
              placeholder={t.onboardingLoginPasswordPlaceholder || '请输入密码'}
            />
          </div>

          <Button
            variant="primary"
            onClick={handlePasswordLogin}
            disabled={loading || !username.trim() || !password}
            data-tauri-drag-region="false"
          >
            {loading ? (
              <>
                <Loader2 size={14} className="animate-spin" />
                {t.onboardingLoginLoggingIn || '登录中...'}
              </>
            ) : (
              t.onboardingLoginSubmit || '登录'
            )}
          </Button>
        </div>
      )}

      <div className="onboarding-login-mode-switch">
        <button
          type="button"
          className="onboarding-link"
          onClick={() => {
            setMode(mode === 'device' ? 'password' : 'device')
            setError(null)
            setDeviceFlow(null)
            setPolling(false)
          }}
          disabled={loading || polling}
        >
          {mode === 'device'
            ? t.onboardingLoginSwitchToPassword || '使用密码登录'
            : t.onboardingLoginSwitchToDevice || '使用浏览器登录'}
        </button>
      </div>
    </div>
  )
}

// 获取设备名称（用于显示）
async function getDeviceName(): Promise<string> {
  try {
    // 尝试从 Tauri 获取主机名
    const hostname = await api.getHostname?.()
    if (hostname) return hostname
  } catch {
    // Fallback
  }

  // 使用 platform + 时间戳作为默认名称
  const platform =
    typeof navigator !== 'undefined'
      ? navigator.platform || navigator.userAgent
      : 'Unknown'
  return `${platform} - ${new Date().toLocaleDateString()}`
}
