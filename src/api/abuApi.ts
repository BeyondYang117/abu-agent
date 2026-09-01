/**
 * ABU API 客户端
 *
 * 对接 abu-api (https://github.com/BeyondYang117/abu-api) 的 Agent 认证、
 * 设备管理、模型列表和中转请求接口。
 */

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
}

export interface AgentSession {
  id: string
  user_id: number
  device_id: string
  created_at: number
  updated_at: number
}

export interface AgentTask {
  id: string
  user_id: number
  session_id: string
  device_id: string
  type: string
  status: 'running' | 'succeeded' | 'failed' | 'cancelled'
  soft_cap: number
  hard_cap: number
  consumed_quota: number
  fail_reason?: string
  created_at: number
  updated_at: number
  finished_at?: number
}

export interface AgentTaskUsage {
  task_id: string
  prompt_tokens: number
  output_tokens: number
  consumed_quota: number
  soft_cap: number
  hard_cap: number
  soft_cap_exceeded: boolean
  hard_cap_exceeded: boolean
}

export interface RelaySession {
  relay_key: string
  expires_at: number
  device_id: string
  task_id: string
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
    return this.request<AgentDevice[]>('/api/agent/devices')
  }

  /**
   * 吊销设备
   */
  async revokeDevice(deviceId: string): Promise<void> {
    await this.request<void>(`/api/agent/devices/${deviceId}`, {
      method: 'DELETE',
    })
  }

  // ==================== 模型与会话 ====================

  /**
   * 获取当前用户可用的模型列表
   */
  async listModels(): Promise<AgentModelsResponse> {
    return this.request<AgentModelsResponse>('/api/agent/models')
  }

  /**
   * 创建 Agent Session
   */
  async createSession(deviceId: string): Promise<AgentSession> {
    return this.request<AgentSession>(`/api/agent/sessions?device_id=${deviceId}`, {
      method: 'POST',
    })
  }

  /**
   * 创建中转 Session（用于调用 /api/agent/relay/* 端点）
   */
  async createRelaySession(deviceId: string, taskId: string): Promise<RelaySession> {
    return this.request<RelaySession>(
      `/api/agent/relay-session?device_id=${deviceId}&task_id=${taskId}`,
      { method: 'POST' },
    )
  }

  // ==================== Task 管理 ====================

  /**
   * 创建 Agent Task
   */
  async createTask(params: {
    session_id: string
    device_id: string
    type: string
    soft_cap?: number
    hard_cap?: number
  }): Promise<AgentTask> {
    return this.request<AgentTask>('/api/agent/tasks', {
      method: 'POST',
      body: JSON.stringify(params),
    })
  }

  /**
   * 列出 Tasks
   */
  async listTasks(params?: {
    limit?: number
    offset?: number
    status?: string
    device_id?: string
  }): Promise<AgentTask[]> {
    const query = new URLSearchParams()
    if (params?.limit) query.set('limit', String(params.limit))
    if (params?.offset) query.set('offset', String(params.offset))
    if (params?.status) query.set('status', params.status)
    if (params?.device_id) query.set('device_id', params.device_id)

    const path = `/api/agent/tasks${query.toString() ? `?${query}` : ''}`
    return this.request<AgentTask[]>(path)
  }

  /**
   * 获取 Task 详情
   */
  async getTask(taskId: string): Promise<AgentTask> {
    return this.request<AgentTask>(`/api/agent/tasks/${taskId}`)
  }

  /**
   * 获取 Task 用量
   */
  async getTaskUsage(taskId: string): Promise<AgentTaskUsage> {
    return this.request<AgentTaskUsage>(`/api/agent/tasks/${taskId}/usage`)
  }

  /**
   * 更新 Task 状态
   */
  async updateTaskStatus(
    taskId: string,
    from: string,
    to: string,
    reason?: string,
  ): Promise<void> {
    await this.request<void>(`/api/agent/tasks/${taskId}/status`, {
      method: 'POST',
      body: JSON.stringify({ from, to, reason }),
    })
  }

  /**
   * 心跳（防止 Task 被判定为僵尸任务）
   */
  async heartbeatTask(taskId: string): Promise<void> {
    await this.request<void>(`/api/agent/tasks/${taskId}/heartbeat`, {
      method: 'POST',
    })
  }

  /**
   * 取消 Task
   */
  async cancelTask(taskId: string): Promise<void> {
    await this.request<void>(`/api/agent/tasks/${taskId}/cancel`, {
      method: 'POST',
    })
  }

  /**
   * 删除 Task
   */
  async deleteTask(taskId: string): Promise<void> {
    await this.request<void>(`/api/agent/tasks/${taskId}`, {
      method: 'DELETE',
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
    throw new Error('ABU API client not initialized')
  }
  return defaultClient
}

export function isAbuApiInitialized(): boolean {
  return defaultClient !== null
}
