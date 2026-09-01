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

// 完成登录流程（在 Onboarding 中调用）
export async function completeLogin(sessionToken: string): Promise<void> {
  const { api } = await import('./tauri')
  const { DEFAULT_ABU_API_BASE_URL } = await import('./abuApi')
  const deviceId = await api.getDeviceFingerprint()
  const baseUrl = abuApiAuthStore.getState().baseUrl || DEFAULT_ABU_API_BASE_URL

  // 保存到 settings
  await api.saveAbuApiConfig({
    base_url: baseUrl,
    session_token: sessionToken,
    device_id: deviceId,
    runtime_mode: 'cloud',
  })

  // 更新内存状态
  abuApiAuthStore.login(sessionToken, deviceId, baseUrl)
}
