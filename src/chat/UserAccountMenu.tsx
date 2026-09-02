import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { LogOut, User, Mail, Coins, Crown, Calendar, Settings, Gift, TrendingUp } from 'lucide-react'
import { useCloseAnimation } from './useCloseAnimation'
import { i18n, type Lang } from '../settings/i18n'

interface AccountInfo {
  username: string
  displayName?: string
  email?: string
  quota: number
  usedQuota: number
  group: string
}

interface UserAccountMenuProps {
  triggerRect: { left: number; top: number; width: number }
  lang: Lang
  accountInfo: AccountInfo | null
  loading: boolean
  onOpenSettings: () => void
  onLogout: () => void
  onClose: () => void
}

export function UserAccountMenu({
  triggerRect,
  lang,
  accountInfo,
  loading,
  onOpenSettings,
  onLogout,
  onClose: onCloseProp,
}: UserAccountMenuProps) {
  const t = i18n[lang]
  const menuRef = useRef<HTMLDivElement>(null)
  const { closing, startClose, onAnimationEnd } = useCloseAnimation(onCloseProp)
  const onClose = startClose

  useEffect(() => {
    const onPointerDown = (e: MouseEvent) => {
      if (menuRef.current?.contains(e.target as Node)) return
      onClose()
    }
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('mousedown', onPointerDown)
    window.addEventListener('keydown', onKeyDown)
    return () => {
      window.removeEventListener('mousedown', onPointerDown)
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [onClose])

  const balance = accountInfo ? (accountInfo.quota / 100).toFixed(2) : '0.00'
  const usedBalance = accountInfo ? (accountInfo.usedQuota / 100).toFixed(2) : '0.00'
  const isVip = accountInfo?.group === 'vip'

  const menu = (
    <div
      ref={menuRef}
      className={`kv-menu ${
        closing ? 'chat-motion-popover-out' : 'chat-motion-popover chat-motion-menu-cascade'
      } fixed z-[200] min-w-[240px]`}
      style={{
        left: triggerRect.left,
        bottom: window.innerHeight - triggerRect.top + 6,
        width: Math.max(triggerRect.width, 240),
        ['--chat-popover-origin' as string]: 'bottom left',
      }}
      role="menu"
      onAnimationEnd={onAnimationEnd}
    >
      {/* 用户信息卡片 */}
      {accountInfo && (
        <div className="px-3 py-2.5 border-b border-neutral-200 dark:border-neutral-700">
          <div className="flex items-center gap-2 mb-2">
            <User size={14} className="text-neutral-500 dark:text-neutral-400" />
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-1.5">
                <span className="text-sm font-medium text-neutral-900 dark:text-neutral-100 truncate">
                  {accountInfo.displayName || accountInfo.username}
                </span>
                {isVip && <Crown size={12} className="shrink-0 text-yellow-500" />}
              </div>
              {accountInfo.displayName && accountInfo.displayName !== accountInfo.username && (
                <div className="text-xs text-neutral-500 dark:text-neutral-400 truncate">
                  @{accountInfo.username}
                </div>
              )}
            </div>
          </div>

          {accountInfo.email && (
            <div className="flex items-center gap-2 mb-2 text-xs text-neutral-600 dark:text-neutral-400">
              <Mail size={12} className="shrink-0" />
              <span className="truncate">{accountInfo.email}</span>
            </div>
          )}

          <div className="flex items-center gap-2 text-xs">
            <Coins size={12} className="shrink-0 text-neutral-500 dark:text-neutral-400" />
            <div className="flex-1">
              <div className="flex items-baseline gap-1">
                <span className="text-sm font-semibold text-neutral-900 dark:text-neutral-100 tabular-nums">
                  ${balance}
                </span>
                <span className="text-neutral-500 dark:text-neutral-400">
                  {lang === 'zh' ? '余额' : 'Balance'}
                </span>
              </div>
              <div className="text-[10px] text-neutral-500 dark:text-neutral-400">
                {lang === 'zh' ? '已用' : 'Used'} ${usedBalance}
              </div>
            </div>
          </div>
        </div>
      )}

      {loading && !accountInfo && (
        <div className="px-3 py-2 text-xs text-neutral-500 dark:text-neutral-400 text-center">
          {lang === 'zh' ? '加载中...' : 'Loading...'}
        </div>
      )}

      <div className="kv-menu-sep" />

      {/* 签到按钮 - 未来功能 */}
      <button
        type="button"
        role="menuitem"
        className="kv-menu-item"
        onClick={() => {
          // TODO: 实现签到功能
          console.log('签到功能即将上线')
          onClose()
        }}
      >
        <Calendar strokeWidth={1.75} />
        {lang === 'zh' ? '每日签到' : 'Daily Check-in'}
        <span className="ml-auto text-xs text-neutral-400 dark:text-neutral-500">
          {lang === 'zh' ? '即将上线' : 'Coming soon'}
        </span>
      </button>

      {/* 升级按钮 - 未来功能 */}
      {!isVip && (
        <button
          type="button"
          role="menuitem"
          className="kv-menu-item"
          onClick={() => {
            // TODO: 实现升级功能
            console.log('升级功能即将上线')
            onClose()
          }}
        >
          <TrendingUp strokeWidth={1.75} />
          {lang === 'zh' ? '升级会员' : 'Upgrade'}
          <Crown size={14} className="ml-auto text-yellow-500" />
        </button>
      )}

      {/* 兑换码 - 未来功能 */}
      <button
        type="button"
        role="menuitem"
        className="kv-menu-item"
        onClick={() => {
          // TODO: 实现兑换码功能
          console.log('兑换码功能即将上线')
          onClose()
        }}
      >
        <Gift strokeWidth={1.75} />
        {lang === 'zh' ? '兑换码' : 'Redeem Code'}
      </button>

      <div className="kv-menu-sep" />

      {/* 账户设置 */}
      <button
        type="button"
        role="menuitem"
        className="kv-menu-item"
        onClick={() => {
          onOpenSettings()
          onClose()
        }}
      >
        <Settings strokeWidth={1.75} />
        {lang === 'zh' ? '账户设置' : 'Account Settings'}
      </button>

      {/* 退出登录 */}
      <button
        type="button"
        role="menuitem"
        className="kv-menu-item text-red-600 dark:text-red-400"
        onClick={() => {
          onLogout()
          onClose()
        }}
      >
        <LogOut strokeWidth={1.75} />
        {lang === 'zh' ? '退出登录' : 'Log Out'}
      </button>
    </div>
  )

  return createPortal(menu, document.body)
}
