import { beforeEach, describe, expect, it, vi } from 'vitest'

const {
  isTauriRuntimeMock,
  abuApiListModelsMock,
  abuApiListEntitlementsMock,
  abuApiListDevicesMock,
  abuApiRevokeDeviceMock,
  abuApiListTasksMock,
  abuApiGetTaskUsageMock,
  abuApiListTaskAttemptsMock,
} = vi.hoisted(() => ({
  isTauriRuntimeMock: vi.fn(),
  abuApiListModelsMock: vi.fn(),
  abuApiListEntitlementsMock: vi.fn(),
  abuApiListDevicesMock: vi.fn(),
  abuApiRevokeDeviceMock: vi.fn(),
  abuApiListTasksMock: vi.fn(),
  abuApiGetTaskUsageMock: vi.fn(),
  abuApiListTaskAttemptsMock: vi.fn(),
}))

vi.mock('./tauri', () => ({
  api: {
    abuApiListModels: abuApiListModelsMock,
    abuApiListEntitlements: abuApiListEntitlementsMock,
    abuApiListDevices: abuApiListDevicesMock,
    abuApiRevokeDevice: abuApiRevokeDeviceMock,
    abuApiListTasks: abuApiListTasksMock,
    abuApiGetTaskUsage: abuApiGetTaskUsageMock,
    abuApiListTaskAttempts: abuApiListTaskAttemptsMock,
  },
  isTauriRuntime: isTauriRuntimeMock,
}))

describe('AbuApiClient.listModels', () => {
  beforeEach(() => {
    vi.resetModules()
    isTauriRuntimeMock.mockReset()
    abuApiListModelsMock.mockReset()
    abuApiListEntitlementsMock.mockReset()
    abuApiListDevicesMock.mockReset()
    abuApiRevokeDeviceMock.mockReset()
    abuApiListTasksMock.mockReset()
    abuApiGetTaskUsageMock.mockReset()
    abuApiListTaskAttemptsMock.mockReset()
    vi.stubGlobal('fetch', vi.fn())
  })

  it('uses the native Tauri request in the desktop runtime', async () => {
    isTauriRuntimeMock.mockReturnValue(true)
    abuApiListModelsMock.mockResolvedValue({ models: ['gpt-4o'], recommended: 'gpt-4o' })

    const { AbuApiClient } = await import('./abuApi')
    const result = await new AbuApiClient('https://api.example.com', 'session-token').listModels()

    expect(result).toEqual({ models: ['gpt-4o'], recommended: 'gpt-4o' })
    expect(abuApiListModelsMock).toHaveBeenCalledOnce()
    expect(fetch).not.toHaveBeenCalled()
  })

  it('uses the native Tauri request for subscription entitlements', async () => {
    isTauriRuntimeMock.mockReturnValue(true)
    const entitlements = [{ id: 1, plan_name: 'Pro' }]
    abuApiListEntitlementsMock.mockResolvedValue(entitlements)

    const { AbuApiClient } = await import('./abuApi')
    const result = await new AbuApiClient('https://api.example.com', 'session-token').listEntitlements()

    expect(result).toEqual(entitlements)
    expect(abuApiListEntitlementsMock).toHaveBeenCalledOnce()
    expect(fetch).not.toHaveBeenCalled()
  })

  it('uses native requests for devices and tasks in the desktop runtime', async () => {
    isTauriRuntimeMock.mockReturnValue(true)
    abuApiListDevicesMock.mockResolvedValue([
      { id: 'device-1', status: 'active', revoked_at: null },
    ])
    abuApiListTasksMock.mockResolvedValue([
      { id: 'task-1', status: 'succeeded', subscription_id: null },
    ])
    abuApiGetTaskUsageMock.mockResolvedValue({
      task_id: 'task-1',
      prompt_tokens: 2,
      completion_tokens: 3,
      consumed_quota: 4,
      soft_cap: 5,
      hard_cap: 6,
      soft_cap_exceeded: false,
      hard_cap_exceeded: false,
      status: 'succeeded',
    })
    abuApiListTaskAttemptsMock.mockResolvedValue([])

    const { AbuApiClient } = await import('./abuApi')
    const client = new AbuApiClient('https://api.example.com', 'session-token')

    await expect(client.listDevices()).resolves.toMatchObject([{ id: 'device-1', revoked_at: undefined }])
    await expect(client.listTasks({ limit: 10 })).resolves.toMatchObject([{ id: 'task-1', user_id: undefined }])
    await expect(client.getTaskUsage('task-1')).resolves.toMatchObject({ completion_tokens: 3, output_tokens: 3 })
    await expect(client.listTaskAttempts('task-1')).resolves.toEqual([])
    await client.revokeDevice('device-1')

    expect(abuApiListDevicesMock).toHaveBeenCalledOnce()
    expect(abuApiListTasksMock).toHaveBeenCalledWith({ limit: 10 })
    expect(abuApiGetTaskUsageMock).toHaveBeenCalledWith('task-1')
    expect(abuApiListTaskAttemptsMock).toHaveBeenCalledWith('task-1')
    expect(abuApiRevokeDeviceMock).toHaveBeenCalledWith('device-1')
    expect(fetch).not.toHaveBeenCalled()
  })
})
