import { beforeEach, describe, expect, it, vi } from 'vitest'

const {
  isTauriRuntimeMock,
  abuApiListModelsMock,
  abuApiListEntitlementsMock,
  abuApiListDevicesMock,
  abuApiRevokeDeviceMock,
  abuApiGetCheckinStatsMock,
  abuApiCheckinMock,
} = vi.hoisted(() => ({
  isTauriRuntimeMock: vi.fn(),
  abuApiListModelsMock: vi.fn(),
  abuApiListEntitlementsMock: vi.fn(),
  abuApiListDevicesMock: vi.fn(),
  abuApiRevokeDeviceMock: vi.fn(),
  abuApiGetCheckinStatsMock: vi.fn(),
  abuApiCheckinMock: vi.fn(),
}))

vi.mock('./tauri', () => ({
  api: {
    abuApiListModels: abuApiListModelsMock,
    abuApiListEntitlements: abuApiListEntitlementsMock,
    abuApiListDevices: abuApiListDevicesMock,
    abuApiRevokeDevice: abuApiRevokeDeviceMock,
    abuApiGetCheckinStats: abuApiGetCheckinStatsMock,
    abuApiCheckin: abuApiCheckinMock,
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
    abuApiGetCheckinStatsMock.mockReset()
    abuApiCheckinMock.mockReset()
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

  it('uses the native device request in the desktop runtime', async () => {
    isTauriRuntimeMock.mockReturnValue(true)
    abuApiListDevicesMock.mockResolvedValue([
      { id: 'device-1', status: 'active', revoked_at: null },
    ])
    const { AbuApiClient } = await import('./abuApi')
    const client = new AbuApiClient('https://api.example.com', 'session-token')

    await expect(client.listDevices()).resolves.toMatchObject([{ id: 'device-1', revoked_at: undefined }])
    await client.revokeDevice('device-1')

    expect(abuApiListDevicesMock).toHaveBeenCalledOnce()
    expect(abuApiRevokeDeviceMock).toHaveBeenCalledWith('device-1')
    expect(fetch).not.toHaveBeenCalled()
  })

  it('uses native desktop requests for check-in status and execution', async () => {
    isTauriRuntimeMock.mockReturnValue(true)
    abuApiGetCheckinStatsMock.mockResolvedValue({ consecutive_days: 4, checked_in_today: false })
    abuApiCheckinMock.mockResolvedValue({ success: true, total_reward: 1000, consecutive_days: 5 })
    const { AbuApiClient } = await import('./abuApi')
    const client = new AbuApiClient('https://api.example.com', 'session-token')

    await expect(client.getCheckinStats()).resolves.toMatchObject({ consecutive_days: 4 })
    await expect(client.checkin()).resolves.toMatchObject({ total_reward: 1000 })

    expect(abuApiGetCheckinStatsMock).toHaveBeenCalledOnce()
    expect(abuApiCheckinMock).toHaveBeenCalledOnce()
    expect(fetch).not.toHaveBeenCalled()
  })

  it('uses the canonical user check-in endpoints in the web runtime', async () => {
    isTauriRuntimeMock.mockReturnValue(false)
    vi.stubGlobal('window', { setTimeout, clearTimeout })
    vi.mocked(fetch)
      .mockResolvedValueOnce(new Response(JSON.stringify({ success: true, data: { checked_in_today: false } }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ success: true, data: { total_reward: 1000 } }), { status: 200 }))
    const { AbuApiClient } = await import('./abuApi')
    const client = new AbuApiClient('https://api.abuai.chat', 'session-token')

    await client.getCheckinStats()
    await client.checkin()

    expect(fetch).toHaveBeenNthCalledWith(1, 'https://api.abuai.chat/api/user/checkin/stats', expect.any(Object))
    expect(fetch).toHaveBeenNthCalledWith(2, 'https://api.abuai.chat/api/user/checkin', expect.any(Object))
  })
})
