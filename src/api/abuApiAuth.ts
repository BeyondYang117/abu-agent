/**
 * ABU API 认证状态管理
 *
 * 提供统一的认证状态访问、session token 存储和设备注册。
 */

import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { AbuApiClient, initAbuApiClient, type AgentDevice } from './abuApi'
import { api } from './tauri'

export interface AbuApiAuthState {
  // 认证状态
  isAuthenticated: boolean
  sessionToken: string | null
  deviceId: string | null
  baseUrl: string

  // 用户信息（可选，需要额外接口）
  user: {
    id: number
    username: string
    email?: string
    quota: number
    group: string
  } | null

  // 设备信息
  currentDevice: AgentDevice | null

  // 操作
  setSessionToken: (token: string) => void
  setDeviceId: (deviceId: string) => void
  setUser: (user: AbuApiAuthState['user']) => void
  setCurrentDevice: (device: AgentDevice) => void
  logout: () => Promise<void>
  initialize: () => Promise<void>
}

export const useAbuApiAuth = create<AbuApiAuthState>()(
  persist(
    (set, get) => ({
      isAuthenticated: false,
      sessionToken: null,
      deviceId: null,
      baseUrl: 'https://api.abuai.com',
      user: null,
      currentDevice: null,

      setSessionToken: (token: string) => {
        set({ sessionToken: token, isAuthenticated: true })
        initAbuApiClient(get().baseUrl, token)
      },

      setDeviceId: (deviceId: string) => {
        set({ deviceId })
      },

      setUser: (user) => {
        set({ user })
      },

      setCurrentDevice: (device) => {
        set({ currentDevice: device })
      },

      logout: async () => {
        const { sessionToken, baseUrl } = get()
        if (sessionToken) {
          try {
            const client = new AbuApiClient(baseUrl, sessionToken)
            await client.logout()
          } catch (err) {
            console.error('Logout API call failed:', err)
          }
        }

        // 清除本地状态
        set({
          isAuthenticated: false,
          sessionToken: null,
          user: null,
        })

        // 清除 Tauri 存储
        try {
          await api.clearAbuApiSession()
        } catch (err) {
          console.error('Failed to clear Tauri session:', err)
        }

        // 重新初始化客户端（无 token）
        initAbuApiClient(get().baseUrl)
      },

      initialize: async () => {
        try {
          // 从 Tauri settings 加载配置
          const config = await api.loadAbuApiConfig()

          set({
            baseUrl: config.base_url || 'https://api.abuai.com',
            sessionToken: config.session_token || null,
            deviceId: config.device_id || null,
            isAuthenticated: !!config.session_token,
          })

          // 初始化 API 客户端
          initAbuApiClient(config.base_url, config.session_token || undefined)

          // 如果有 session token，尝试注册/更新设备
          if (config.session_token) {
            await registerOrUpdateDevice()
          }
        } catch (err) {
          console.error('Failed to initialize ABU API auth:', err)
        }
      },
    }),
    {
      name: 'abu-api-auth',
      partialize: (state) => ({
        // 只持久化基础配置，敏感信息走 Tauri settings
        baseUrl: state.baseUrl,
      }),
    },
  ),
)

/**
 * 注册或更新当前设备
 */
export async function registerOrUpdateDevice(): Promise<AgentDevice> {
  const { sessionToken, baseUrl, deviceId, setDeviceId, setCurrentDevice } =
    useAbuApiAuth.getState()

  if (!sessionToken) {
    throw new Error('Not authenticated')
  }

  const client = new AbuApiClient(baseUrl, sessionToken)

  // 获取设备信息
  const fingerprint = await api.getDeviceFingerprint()
  const platform = await api.getPlatform()
  const clientVersion = await api.getClientVersion()
  const deviceName = await api.getDefaultDeviceName()

  // 注册/更新设备
  const device = await client.registerDevice({
    fingerprint,
    platform,
    client_version: clientVersion,
    device_name: deviceName,
    capabilities: JSON.stringify({
      features: ['chat', 'translate', 'lens', 'screenshot'],
      protocols: ['openai', 'anthropic', 'gemini'],
    }),
  })

  // 保存设备 ID
  if (device.id !== deviceId) {
    setDeviceId(device.id)
    await api.saveAbuApiConfig({
      base_url: baseUrl,
      session_token: sessionToken,
      device_id: device.id,
    })
  }

  setCurrentDevice(device)
  return device
}

/**
 * 完成登录流程（Device Code 或密码登录后调用）
 */
export async function completeLogin(sessionToken: string): Promise<void> {
  const { setSessionToken, baseUrl } = useAbuApiAuth.getState()

  // 1. 设置 session token
  setSessionToken(sessionToken)

  // 2. 注册设备
  const device = await registerOrUpdateDevice()

  // 3. 保存到 Tauri settings
  await api.saveAbuApiConfig({
    base_url: baseUrl,
    session_token: sessionToken,
    device_id: device.id,
  })

  // 4. 可选：获取用户信息（需要 abu-api 提供 /api/user/me 接口）
  // const user = await client.getUserInfo()
  // setUser(user)
}

/**
 * 检查认证状态是否有效
 */
export async function validateAuth(): Promise<boolean> {
  const { sessionToken, baseUrl, isAuthenticated } = useAbuApiAuth.getState()

  if (!isAuthenticated || !sessionToken) {
    return false
  }

  try {
    // 尝试调用一个需要认证的接口（如获取模型列表）
    const client = new AbuApiClient(baseUrl, sessionToken)
    await client.listModels()
    return true
  } catch (err) {
    console.error('Auth validation failed:', err)
    // Session token 可能已过期，清除状态
    useAbuApiAuth.getState().logout()
    return false
  }
}
