/**
 * ABU API 认证状态管理（无第三方依赖，纯 React）
 */

import { useEffect, useState } from 'react'

export interface AbuApiAuthState {
  isLoggedIn: boolean
  isAuthenticated: boolean // alias for isLoggedIn
  sessionToken: string | null
  deviceId: string | null
  baseUrl: string | null
}

type Listener = () => void

class AbuApiAuthStore {
  private state: AbuApiAuthState = {
    isLoggedIn: false,
    isAuthenticated: false,
    sessionToken: null,
    deviceId: null,
    baseUrl: null,
  }

  private listeners = new Set<Listener>()

  getState(): AbuApiAuthState {
    return this.state
  }

  setState(partial: Partial<AbuApiAuthState>) {
    this.state = { ...this.state, ...partial }
    this.emit()
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  private emit() {
    this.listeners.forEach((listener) => listener())
  }

  // 业务方法
  login(sessionToken: string, deviceId: string, baseUrl: string) {
    this.setState({
      isLoggedIn: true,
      isAuthenticated: true,
      sessionToken,
      deviceId,
      baseUrl,
    })
  }

  logout() {
    this.setState({
      isLoggedIn: false,
      isAuthenticated: false,
      sessionToken: null,
      // deviceId 保留，供下次登录复用
    })
  }

  updateFromSettings(config: {
    sessionToken?: string | null
    deviceId?: string | null
    baseUrl?: string | null
  }) {
    const authenticated = !!config.sessionToken
    this.setState({
      isLoggedIn: authenticated,
      isAuthenticated: authenticated,
      sessionToken: config.sessionToken || null,
      deviceId: config.deviceId || null,
      baseUrl: config.baseUrl || null,
    })
  }
}

export const abuApiAuthStore = new AbuApiAuthStore()

// React Hook (类似 zustand 的 API)
export function useAbuApiAuth(): AbuApiAuthState {
  const [state, setState] = useState(abuApiAuthStore.getState())

  useEffect(() => {
    const unsubscribe = abuApiAuthStore.subscribe(() => {
      setState(abuApiAuthStore.getState())
    })
    return unsubscribe
  }, [])

  return state
}

// 为了兼容可能的旧代码，导出一个 actions 对象
export const abuApiAuthActions = {
  login: (sessionToken: string, deviceId: string, baseUrl: string) =>
    abuApiAuthStore.login(sessionToken, deviceId, baseUrl),
  logout: () => abuApiAuthStore.logout(),
  updateFromSettings: (config: {
    sessionToken?: string | null
    deviceId?: string | null
    baseUrl?: string | null
  }) => abuApiAuthStore.updateFromSettings(config),
}

/** 切换 ABU API 域名并持久化；会立即更新内存客户端与认证状态。 */
export async function switchAbuApiBaseUrl(baseUrl: string): Promise<void> {
  const { api } = await import('./tauri')
  const { initAbuApiClient } = await import('./abuApi')
  const { normalizeAbuApiBaseUrl } = await import('./abuApiEndpoints')
  const normalized = normalizeAbuApiBaseUrl(baseUrl)
  const current = abuApiAuthStore.getState()
  const config = await api.loadAbuApiConfig()
  await api.saveAbuApiConfig({
    base_url: normalized,
    session_token: current.sessionToken || config.session_token,
    device_id: current.deviceId || config.device_id,
    runtime_mode: config.runtime_mode,
  })
  initAbuApiClient(normalized, current.sessionToken || config.session_token || undefined)
  abuApiAuthStore.setState({ baseUrl: normalized })
  try {
    const { refreshSettings } = await import('./settingsCache')
    await refreshSettings()
  } catch {
    // Settings refresh is best effort; native config is already persisted.
  }
}

// 完成登录流程（在 Onboarding 中调用）
export async function completeLogin(sessionToken: string): Promise<void> {
  const { api, isTauriRuntime } = await import('./tauri')
  const { DEFAULT_ABU_API_BASE_URL, initAbuApiClient, AbuApiClient } = await import('./abuApi')
  const fingerprint = await api.getDeviceFingerprint()
  const baseUrl = abuApiAuthStore.getState().baseUrl || DEFAULT_ABU_API_BASE_URL

  // 初始化临时客户端用于注册设备
  const client = new AbuApiClient(baseUrl, sessionToken)

  // 注册/更新设备（幂等操作，相同 fingerprint 会自动更新）
  const platform = await api.getPlatform()
  const clientVersion = await api.getClientVersion()
  const deviceName = await api.getDefaultDeviceName()

  const capabilities = JSON.stringify({
    platform,
    version: clientVersion,
  })
  const device = isTauriRuntime()
    ? await api.abuApiRegisterDevice({
        baseUrl,
        sessionToken,
        fingerprint,
        platform,
        clientVersion,
        deviceName,
        capabilities,
      })
    : await client.registerDevice({
        fingerprint,
        platform,
        client_version: clientVersion,
        device_name: deviceName,
        capabilities,
      })

  // 使用服务端返回的 device.id 而不是本地指纹
  const deviceId = device.id

  // 保存到 settings
  await api.saveAbuApiConfig({
    base_url: baseUrl,
    session_token: sessionToken,
    device_id: deviceId,
    runtime_mode: 'cloud',
  })

  // 刷新前端 settings 缓存并广播给所有监听者
  try {
    const { refreshSettings } = await import('./settingsCache')
    await refreshSettings()
  } catch (err) {
    console.warn('Failed to refresh settings cache:', err)
  }

  // 初始化 ABU API 客户端
  initAbuApiClient(baseUrl, sessionToken)

  // 更新内存状态
  abuApiAuthStore.login(sessionToken, deviceId, baseUrl)
  const { syncModelRoutingPolicy } = await import('../chat/modelRoutingPolicy')
  void syncModelRoutingPolicy()
}

// 退出登录
export async function logout(): Promise<void> {
  const { api } = await import('./tauri')
  try {
    await api.clearAbuApiSession()
  } catch (err) {
    console.error('Failed to clear Abu API session in backend:', err)
  }
  try {
    const { refreshSettings } = await import('./settingsCache')
    await refreshSettings()
  } catch (err) {
    console.warn('Failed to refresh settings cache after logout:', err)
  }
  abuApiAuthStore.logout()
}
