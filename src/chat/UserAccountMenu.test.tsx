import { fireEvent, render, screen } from '@testing-library/react'
import type { ComponentProps } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { UserAccountMenu } from './UserAccountMenu'

function renderMenu(overrides: Partial<ComponentProps<typeof UserAccountMenu>> = {}) {
  const props: ComponentProps<typeof UserAccountMenu> = {
    triggerRect: { left: 12, top: 600, width: 250 },
    lang: 'zh',
    accountInfo: { username: 'abu', quota: 500_000, temporaryQuota: 0, usedQuota: 100_000, group: 'default' },
    loading: false,
    checkinStats: { consecutive_days: 3, total_checkins: 9, total_quota: 3000, checked_in_today: false },
    checkinLoading: false,
    checkinError: null,
    checkinReward: null,
    onCheckin: vi.fn(),
    onRecharge: vi.fn(),
    onOpenSettings: vi.fn(),
    onLogout: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  }
  render(<UserAccountMenu {...props} />)
  return props
}

describe('UserAccountMenu', () => {
  it('shows the streak and performs daily check-in', () => {
    const props = renderMenu()

    expect(screen.getByText('连续 3 天')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '签到' }))
    expect(props.onCheckin).toHaveBeenCalledOnce()
  })

  it('disables check-in after it has completed', () => {
    renderMenu({
      checkinStats: { consecutive_days: 4, total_checkins: 10, total_quota: 4000, checked_in_today: true },
      checkinReward: 1000,
    })

    expect(screen.getByRole('button', { name: '已签到' })).toBeDisabled()
    expect(screen.getByText('· +$0.002')).toBeInTheDocument()
  })

  it('opens the abu-api top-up flow from the balance action', () => {
    const props = renderMenu()

    fireEvent.click(screen.getByRole('button', { name: '充值' }))
    expect(props.onRecharge).toHaveBeenCalledOnce()
  })
})
