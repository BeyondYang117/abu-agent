import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { LogOut, Coins, Crown, CalendarCheck2, Settings, CreditCard, LoaderCircle, Check, Flame } from 'lucide-react'
import { useCloseAnimation } from './useCloseAnimation'
import { type Lang } from '../settings/i18n'
import { formatAbuQuota, formatAbuQuotaReward } from '../api/quota'
import type { CheckinStats } from '../api/abuApi'

interface AccountInfo {
  username: string
  displayName?: string
  email?: string
  quota: number
  temporaryQuota: number
  usedQuota: number
  group: string
}

interface UserAccountMenuProps {
  triggerRect: { left: number; top: number; width: number }
  lang: Lang
  accountInfo: AccountInfo | null
  loading: boolean
  checkinStats: CheckinStats | null
  checkinLoading: boolean
  checkinError: string | null
  checkinReward: number | null
  onCheckin: () => void
  onRecharge: () => void
  onOpenSettings: () => void
  onLogout: () => void
  onClose: () => void
}

export function UserAccountMenu({
  triggerRect,
  lang,
  accountInfo,
  loading,
  checkinStats,
  checkinLoading,
  checkinError,
  checkinReward,
  onCheckin,
  onRecharge,
  onOpenSettings,
  onLogout,
  onClose: onCloseProp,
}: UserAccountMenuProps) {
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

  const balance = accountInfo ? formatAbuQuota(accountInfo.quota + accountInfo.temporaryQuota) : '0.00'
  const usedBalance = accountInfo ? formatAbuQuota(accountInfo.usedQuota) : '0.00'
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
      {accountInfo ? (
        <div className="px-3 py-2.5 border-b border-neutral-200 dark:border-neutral-700 bg-neutral-50/50 dark:bg-neutral-800/30">
          <div className="flex items-center gap-2.5 mb-2.5">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-gradient-to-br from-blue-500 to-indigo-600 text-white font-semibold text-xs shadow-xs select-none">
              {(accountInfo.displayName || accountInfo.username || 'U').charAt(0).toUpperCase()}
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-1.5">
                <span className="text-sm font-semibold text-neutral-900 dark:text-neutral-100 truncate">
                  {accountInfo.displayName || accountInfo.username}
                </span>
                {isVip && <Crown size={13} className="shrink-0 text-yellow-500" />}
              </div>
              {accountInfo.displayName && accountInfo.displayName !== accountInfo.username ? (
                <div className="text-[11px] text-neutral-500 dark:text-neutral-400 truncate">
                  @{accountInfo.username}
                </div>
              ) : accountInfo.email ? (
                <div className="text-[11px] text-neutral-500 dark:text-neutral-400 truncate">
                  {accountInfo.email}
                </div>
              ) : null}
            </div>
          </div>

          <div className="flex items-center gap-2 rounded-lg bg-white dark:bg-neutral-800/80 border border-neutral-200/80 dark:border-neutral-700/80 p-2 text-xs">
            <Coins size={14} className="shrink-0 text-amber-500" />
            <div className="flex-1 min-w-0">
              <div className="flex items-baseline gap-1.5">
                <span className="text-sm font-bold text-neutral-900 dark:text-neutral-100 tabular-nums">
                  ${balance}
                </span>
                <span className="text-[11px] text-neutral-500 dark:text-neutral-400">
                  {lang === 'zh' ? '可用余额' : 'Balance'}
                </span>
              </div>
              <div className="text-[10px] text-neutral-500 dark:text-neutral-400">
                {lang === 'zh' ? '已消费' : 'Used'} ${usedBalance}
              </div>
            </div>
            <button
              type="button"
              onClick={() => {
                onRecharge()
                onClose()
              }}
              className="shrink-0 rounded-md bg-neutral-900 px-2 py-1 text-[11px] font-medium text-white transition-opacity hover:opacity-80 dark:bg-neutral-100 dark:text-neutral-900"
            >
              {lang === 'zh' ? '充值' : 'Top up'}
            </button>
          </div>
        </div>
      ) : loading ? (
        <div className="px-3 py-3 border-b border-neutral-200 dark:border-neutral-700 text-xs text-neutral-500 dark:text-neutral-400 text-center flex items-center justify-center gap-2">
          <div className="h-3 w-3 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
          <span>{lang === 'zh' ? '加载账户信息...' : 'Loading account...'}</span>
        </div>
      ) : (
        <div className="px-3 py-2.5 border-b border-neutral-200 dark:border-neutral-700 bg-neutral-50/50 dark:bg-neutral-800/30">
          <div className="flex items-center gap-2">
            <div className="flex h-7 w-7 items-center justify-center rounded-full bg-blue-100 text-blue-600 dark:bg-blue-900/40 dark:text-blue-300 text-xs font-bold">
              A
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-xs font-medium text-neutral-800 dark:text-neutral-200">
                {lang === 'zh' ? '已登录 ABU 账户' : 'ABU Account Connected'}
              </div>
            </div>
          </div>
        </div>
      )}

      <div className="kv-menu-sep" />

      <div className="mx-1 rounded-lg border border-amber-200/80 bg-gradient-to-br from-amber-50 to-orange-50/70 p-2 dark:border-amber-900/60 dark:from-amber-950/30 dark:to-orange-950/20">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-amber-400/20 text-amber-700 dark:text-amber-300">
            {checkinStats?.checked_in_today ? <Check size={15} /> : <CalendarCheck2 size={15} />}
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-xs font-semibold text-neutral-900 dark:text-neutral-100">
              {checkinStats?.checked_in_today
                ? (lang === 'zh' ? '今日已签到' : 'Checked in today')
                : (lang === 'zh' ? '每日签到领额度' : 'Daily quota reward')}
            </div>
            <div className="mt-0.5 flex items-center gap-1 text-[10px] text-neutral-500 dark:text-neutral-400">
              <Flame size={10} className="text-orange-500" />
              {lang === 'zh' ? `连续 ${checkinStats?.consecutive_days ?? 0} 天` : `${checkinStats?.consecutive_days ?? 0}-day streak`}
              {checkinReward != null && (
                <span className="font-medium text-emerald-600 dark:text-emerald-400">
                  · +${formatAbuQuotaReward(checkinReward)}
                </span>
              )}
            </div>
          </div>
          <button
            type="button"
            disabled={loading || checkinLoading || !checkinStats || checkinStats.checked_in_today}
            onClick={onCheckin}
            className="flex h-7 shrink-0 items-center gap-1 rounded-md bg-amber-500 px-2.5 text-[11px] font-semibold text-white shadow-sm transition-colors hover:bg-amber-600 disabled:bg-amber-300 disabled:shadow-none dark:disabled:bg-amber-900/60"
          >
            {(loading || checkinLoading || !checkinStats) && <LoaderCircle size={11} className="animate-spin" />}
            {checkinError && !checkinStats
              ? (lang === 'zh' ? '暂不可用' : 'Unavailable')
              : loading || !checkinStats
              ? (lang === 'zh' ? '加载中' : 'Loading')
              : checkinStats?.checked_in_today
              ? (lang === 'zh' ? '已签到' : 'Done')
              : (lang === 'zh' ? '签到' : 'Check in')}
          </button>
        </div>
        {checkinError && (
          <div role="alert" className="mt-1.5 line-clamp-2 text-[10px] leading-4 text-red-600 dark:text-red-400" title={checkinError}>
            {checkinError}
          </div>
        )}
      </div>

      <button
        type="button"
        role="menuitem"
        className="kv-menu-item"
        onClick={() => {
          onRecharge()
          onClose()
        }}
      >
        <CreditCard strokeWidth={1.75} />
        {lang === 'zh' ? '充值余额' : 'Top up balance'}
        <span className="kv-menu-hint">abu-api ↗</span>
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
