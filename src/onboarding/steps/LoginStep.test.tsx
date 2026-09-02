import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { LoginStep } from './LoginStep'

const { createDeviceAuthorizationMock, openExternalMock } = vi.hoisted(() => ({
  createDeviceAuthorizationMock: vi.fn(),
  openExternalMock: vi.fn(),
}))

vi.mock('../../api/tauri', () => ({
  api: {
    abuApiCreateDeviceAuthorization: createDeviceAuthorizationMock,
    openExternal: openExternalMock,
    getHostname: vi.fn().mockResolvedValue('test-mac'),
  },
  isTauriRuntime: () => true,
}))

describe('LoginStep device authorization', () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    createDeviceAuthorizationMock.mockReset().mockResolvedValue({
      device_code: 'device-code',
      user_code: 'ABC12345',
      verification_uri: '/agent/authorize',
      expires_at: 1_900_000_000,
      interval: 2,
    })
    openExternalMock.mockReset().mockResolvedValue(undefined)
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('opens and reopens the authorization page with user_code prefilled', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime })
    render(
      <LoginStep
        t={{} as never}
        abuApiBaseUrl="https://api.abuai.chat"
        onLoginSuccess={vi.fn()}
      />,
    )

    await user.click(screen.getByRole('button', { name: '在浏览器中登录' }))
    await waitFor(() => {
      expect(openExternalMock).toHaveBeenCalledWith(
        'https://api.abuai.chat/agent/authorize?user_code=ABC12345',
      )
    })

    await user.click(screen.getByRole('button', { name: '重新打开浏览器' }))
    expect(openExternalMock).toHaveBeenLastCalledWith(
      'https://api.abuai.chat/agent/authorize?user_code=ABC12345',
    )
  })
})
