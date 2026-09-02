import { beforeEach, describe, expect, it, vi } from 'vitest'

const { isTauriRuntimeMock, abuApiListModelsMock } = vi.hoisted(() => ({
  isTauriRuntimeMock: vi.fn(),
  abuApiListModelsMock: vi.fn(),
}))

vi.mock('./tauri', () => ({
  api: {
    abuApiListModels: abuApiListModelsMock,
  },
  isTauriRuntime: isTauriRuntimeMock,
}))

describe('AbuApiClient.listModels', () => {
  beforeEach(() => {
    vi.resetModules()
    isTauriRuntimeMock.mockReset()
    abuApiListModelsMock.mockReset()
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
})
