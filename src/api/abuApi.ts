import { api, isTauriRuntime } from './tauri'
import { abuApiAuthStore } from './abuApiAuth'

/**
 * ABU API 客户端
 *
 * 对接 abu-api (https://api.abuai.chat/) 的 Agent 认证、
 * 设备管理、模型列表和中转请求接口。
 */

/**
 * ABU API 生产环境地址。与 Rust 侧
 * `settings_abu_api::DEFAULT_ABU_API_BASE_URL` 保持一致。
 */
export const DEFAULT_ABU_API_BASE_URL = 'https://api.abuai.chat'

/** Cloud 模式的虚拟 Provider ID（与 Rust 侧 ABU_API_PROVIDER_ID 对应） */
export const ABU_API_PROVIDER_ID = 'abu-api-relay'

export interface DeviceAuthResponse {
  device_code: string
  user_code: string
  verification_uri: string
  expires_at: number
  interval: number
}

export interface ExchangeAuthResponse {
  status: 'pending' | 'approved' | 'consumed' | 'denied' | 'expired'
  session_token?: string
}

export interface AgentDevice {
  id: string
  platform: string
  client_version: string
  device_name: string
  capabilities: string
  status: 'active' | 'revoked'
  last_seen_at: number
  created_at: number
  updated_at: number
  revoked_at?: number
}

export interface AgentModelsResponse {
  models: string[]
  recommended: string
  model_access?: AgentModelAccess[]
}

export type AgentModelAccessStatus = 'available' | 'subscription_required' | 'quota_exhausted' | 'unavailable'

export interface AgentModelAccess {
  model: string
  status: AgentModelAccessStatus
  recommended_plan_ids?: number[]
}

export interface ModelRoutingPolicyCache {
  version: number
  fetched_at: number
  payload: unknown
}

export interface AgentEntitlement {
  id: number
  plan_id: number
  plan_name: string
  group_id: number
  supported_models: string[]
  bound_groups: string[]
  daily_limit_usd?: number | null
  weekly_limit_usd?: number | null
  monthly_limit_usd?: number | null
  daily_usage_usd: number
  weekly_usage_usd: number
  monthly_usage_usd: number
  extra_quota_remaining_usd: number
  start_date: string
  end_date: string
}

export interface AgentRelayCredentials {
  api_key: string
  groups: string[]
  models: string[]
  recommended_model: string
}

export interface AbuApiError {
  success: false
  message: string
  data?: unknown
}

export class AbuApiClient {
  constructor(
    private baseUrl: string,
    private sessionToken?: string,
  ) {}

  private async request<T>(
    path: string,
    options: RequestInit = {},
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...(options.headers as Record<string, string>),
    }

    if (this.sessionToken) {
      headers['X-Abu-Session-Token'] = this.sessionToken
    }

    const response = await fetch(url, {
      ...options,
      headers,
    })

    const data = await response.json()

    if (!data.success) {
      const error = data as AbuApiError
      throw new Error(error.message || 'API request failed')
    }

    return data.data as T
  }

  // ==================== 认证相关 ====================

  /**
   * 创建设备授权请求（Device Code Flow）
   */
  async createDeviceAuthorization(deviceName: string): Promise<DeviceAuthResponse> {
    return this.request<DeviceAuthResponse>('/api/agent/auth/device', {
      method: 'POST',
      body: JSON.stringify({ device_name: deviceName }),
    })
  }

  /**
   * 兑换设备授权码（轮询此接口直到返回 session_token）
   */
  async exchangeDeviceAuthorization(deviceCode: string): Promise<ExchangeAuthResponse> {
    return this.request<ExchangeAuthResponse>('/api/agent/auth/device/exchange', {
      method: 'POST',
      body: JSON.stringify({ device_code: deviceCode }),
    })
  }

  /**
   * 密码登录（备选方案）
   */
  async loginWithPassword(username: string, password: string): Promise<{ session_token: string }> {
    return this.request<{ session_token: string }>('/api/agent/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    })
  }

  /**
   * 双因素验证登录
   */
  async loginWith2FA(username: string, password: string, code: string): Promise<{ session_token: string }> {
    return this.request<{ session_token: string }>('/api/agent/auth/login/2fa', {
      method: 'POST',
      body: JSON.stringify({ username, password, code }),
    })
  }

  /**
   * 登出（撤销当前 session token）
   */
  async logout(): Promise<void> {
    await this.request<void>('/api/agent/auth/logout', {
      method: 'POST',
    })
  }

  // ==================== 设备管理 ====================

  /**
   * 注册/更新设备（幂等，相同 fingerprint 自动更新）
   */
  async registerDevice(params: {
    fingerprint: string
    platform: string
    client_version: string
    device_name: string
    capabilities?: string
  }): Promise<AgentDevice> {
    return this.request<AgentDevice>('/api/agent/devices', {
      method: 'POST',
      body: JSON.stringify(params),
    })
  }

  /**
   * 列出当前用户的所有设备
   */
  async listDevices(): Promise<AgentDevice[]> {
    if (isTauriRuntime()) {
      const devices = await api.abuApiListDevices()
      return devices.map((device) => ({ ...device, revoked_at: device.revoked_at ?? undefined }))
    }
    return this.request<AgentDevice[]>('/api/agent/devices')
  }

  /**
   * 吊销设备
   */
  async revokeDevice(deviceId: string): Promise<void> {
    if (isTauriRuntime()) {
      await api.abuApiRevokeDevice(deviceId)
      return
    }
    await this.request<void>(`/api/agent/devices/${deviceId}`, {
      method: 'DELETE',
    })
  }

  // ==================== 用户信息 ====================

  /**
   * 获取当前登录用户的信息
   */
  async getUserInfo(): Promise<{
    id: number
    username: string
    display_name?: string
    email?: string
    quota: number
    used_quota: number
    group: string
  }> {
    if (isTauriRuntime()) {
      // Do not fall back to WebView fetch on desktop: the ABU API session is
      // not represented by browser cookies, and fetch only masks the native
      // error as the unhelpful "Load failed" CORS message.
      return api.abuApiGetUserInfo()
    }
    return this.request('/api/agent/devices?include_account=1')
  }

  // ==================== 模型与会话 ====================

  /**
   * 获取当前用户可用的模型列表
   */
  async listModels(): Promise<AgentModelsResponse> {
    if (isTauriRuntime()) {
      // Keep desktop requests on Rust's network stack to avoid WebView CORS
      // failures being surfaced as the unhelpful "Load failed" message.
      return api.abuApiListModels()
    }
    return this.request<AgentModelsResponse>('/api/agent/models')
  }

  async getCachedModelRoutingPolicy(): Promise<ModelRoutingPolicyCache | null> {
    if (isTauriRuntime()) return api.abuApiGetCachedModelRoutingPolicy()
    return null
  }

  async syncModelRoutingPolicy(): Promise<ModelRoutingPolicyCache> {
    if (isTauriRuntime()) return api.abuApiSyncModelRoutingPolicy()
    return this.request<ModelRoutingPolicyCache>('/api/agent/model-routing-policy')
  }

  /** 获取当前用户可用于 Agent 的有效套餐权益。 */
  async listEntitlements(): Promise<AgentEntitlement[]> {
    if (isTauriRuntime()) {
      return api.abuApiListEntitlements()
    }
    return this.request<AgentEntitlement[]>('/api/agent/entitlements')
  }

  async getRelayCredentials(model = ''): Promise<AgentRelayCredentials> {
    if (isTauriRuntime()) {
      if (!this.sessionToken) throw new Error('Not signed in')
      return api.abuApiGetRelayCredentials(this.baseUrl, this.sessionToken, model)
    }
    return this.request<AgentRelayCredentials>('/api/agent/relay-credentials', {
      method: 'POST',
    })
  }
}

/**
 * 默认客户端实例（在运行时初始化）
 */
let defaultClient: AbuApiClient | null = null

export function initAbuApiClient(baseUrl: string, sessionToken?: string): AbuApiClient {
  defaultClient = new AbuApiClient(baseUrl, sessionToken)
  return defaultClient
}

export function getAbuApiClient(): AbuApiClient {
  if (!defaultClient) {
    const auth = abuApiAuthStore.getState()
    const baseUrl = auth.baseUrl || DEFAULT_ABU_API_BASE_URL
    defaultClient = new AbuApiClient(baseUrl, auth.sessionToken || undefined)
  }
  return defaultClient
}

export function isAbuApiInitialized(): boolean {
  return defaultClient !== null
}

// ==================== 便捷导出函数 ====================

export async function listModels(): Promise<AgentModelsResponse> {
  return getAbuApiClient().listModels()
}

export async function listDevices(): Promise<AgentDevice[]> {
  return getAbuApiClient().listDevices()
}

export async function listEntitlements(): Promise<AgentEntitlement[]> {
  return getAbuApiClient().listEntitlements()
}

export async function revokeDevice(deviceId: string): Promise<void> {
  return getAbuApiClient().revokeDevice(deviceId)
}
