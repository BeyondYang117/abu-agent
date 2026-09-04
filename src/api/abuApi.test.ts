import { beforeEach, describe, expect, it, vi } from 'vitest'

const {
  isTauriRuntimeMock,
  abuApiListModelsMock,
  abuApiListEntitlementsMock,
  abuApiListDevicesMock,
  abuApiRevokeDeviceMock,
} = vi.hoisted(() => ({
  isTauriRuntimeMock: vi.fn(),
  abuApiListModelsMock: vi.fn(),
  abuApiListEntitlementsMock: vi.fn(),
  abuApiListDevicesMock: vi.fn(),
  abuApiRevokeDeviceMock: vi.fn(),
}))

vi.mock('./tauri', () => ({
  api: {
    abuApiListModels: abuApiListModelsMock,
    abuApiListEntitlements: abuApiListEntitlementsMock,
    abuApiListDevices: abuApiListDevicesMock,
    abuApiRevokeDevice: abuApiRevokeDeviceMock,
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
})
