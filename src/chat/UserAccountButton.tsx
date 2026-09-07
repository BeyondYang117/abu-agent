import { memo, useCallback, useEffect, useRef, useState } from 'react'
import { User, Settings, Coins, Crown } from 'lucide-react'
import { useAbuApiAuth } from '../api/abuApiAuth'
import { getAbuApiClient, type CheckinStats } from '../api/abuApi'
import { api } from '../api/tauri'
import { UserAvatar } from './UserAvatar'
import type { ChatUserProfile } from './types'
import { i18n, type Lang } from '../settings/i18n'
import { IconButton } from '../components/Button'
import { UserAccountMenu } from './UserAccountMenu'
import { formatAbuQuota } from '../api/quota'
import { resolveAccountDisplayName } from './accountDisplayName'

interface UserAccountButtonProps {
  profile: ChatUserProfile
  lang: Lang
  onOpenSettings: () => void
  onOpenLogin: () => void
  onLogout: () => void
  settingsActive: boolean
}

interface AccountInfo {
  username: string
  displayName?: string
  email?: string
  quota: number
  temporaryQuota: number
  usedQuota: number
  group: string
}

export const UserAccountButton = memo(function UserAccountButton({
  profile,
  lang,
  onOpenSettings,
  onOpenLogin,
  onLogout,
  settingsActive,
}: UserAccountButtonProps) {
  const t = i18n[lang]
  const { isAuthenticated } = useAbuApiAuth()
  const [accountInfo, setAccountInfo] = useState<AccountInfo | null>(null)
  const [loading, setLoading] = useState(false)
  const [checkinStats, setCheckinStats] = useState<CheckinStats | null>(null)
  const [checkinLoading, setCheckinLoading] = useState(false)
  const [checkinError, setCheckinError] = useState<string | null>(null)
  const [checkinReward, setCheckinReward] = useState<number | null>(null)
  const [menuRect, setMenuRect] = useState<{ left: number; top: number; width: number } | null>(null)
  const rowRef = useRef<HTMLDivElement>(null)
  const mountedRef = useRef(true)

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  const loadAccount = useCallback(async () => {
    setLoading(true)
    setCheckinError(null)
    try {
      const client = getAbuApiClient()
      const [info, stats] = await Promise.all([
        client.getUserInfo(),
        client.getCheckinStats().catch((err) => {
          console.error('Failed to load check-in status:', err)
          if (mountedRef.current) setCheckinError(err instanceof Error ? err.message : String(err))
          return null
        }),
      ])
      if (!mountedRef.current) return
      setAccountInfo({
        username: info.username,
        displayName: info.display_name,
        email: info.email,
        quota: info.quota,
        temporaryQuota: info.temporary_quota ?? 0,
        usedQuota: info.used_quota,
        group: info.group,
      })
      setCheckinStats(stats)
    } catch (err) {
      console.error('Failed to load account info:', err)
      if (!mountedRef.current) return
      setAccountInfo(null)
      setCheckinStats(null)
    } finally {
      if (mountedRef.current) setLoading(false)
    }
  }, [])

  useEffect(() => {
    if (isAuthenticated) {
      void loadAccount()
    } else {
      setAccountInfo(null)
      setCheckinStats(null)
    }
  }, [isAuthenticated, loadAccount])

  // Reopening the menu refreshes both a completed web top-up and a new calendar day.
  useEffect(() => {
    if (menuRect && isAuthenticated) void loadAccount()
  }, [isAuthenticated, loadAccount, menuRect])

  const handleCheckin = useCallback(async () => {
    if (checkinLoading || checkinStats?.checked_in_today) return
    setCheckinLoading(true)
    setCheckinError(null)
    setCheckinReward(null)
    try {
      const client = getAbuApiClient()
      const result = await client.checkin()
      const [info, stats] = await Promise.all([client.getUserInfo(), client.getCheckinStats()])
      if (!mountedRef.current) return
      setAccountInfo({
        username: info.username,
        displayName: info.display_name,
        email: info.email,
        quota: info.quota,
        temporaryQuota: info.temporary_quota ?? 0,
        usedQuota: info.used_quota,
        group: info.group,
      })
      setCheckinStats(stats)
      setCheckinReward(result.total_reward)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      if (message.includes('今日已签到') || message.toLowerCase().includes('already checked in')) {
        try {
          const stats = await getAbuApiClient().getCheckinStats()
          if (mountedRef.current) {
            setCheckinStats({ ...stats, checked_in_today: true })
            setCheckinError(null)
          }
          return
        } catch {
          if (mountedRef.current) {
            setCheckinStats((current) => current ? { ...current, checked_in_today: true } : current)
            setCheckinError(null)
          }
          return
        }
      }
      if (mountedRef.current) setCheckinError(message)
    } finally {
      if (mountedRef.current) setCheckinLoading(false)
    }
  }, [checkinLoading, checkinStats?.checked_in_today])

  const openTopup = useCallback(() => {
    const baseUrl = getAbuApiClient().getBaseUrl().replace(/\/+$/, '')
    void api.openExternal(`${baseUrl}/console/topup?source=desktop_account_menu`)
      .catch((err) => console.error('Failed to open top-up page:', err))
  }, [])

  const toggleMenu = useCallback(() => {
    if (!isAuthenticated) {
      onOpenLogin()
      return
    }

    if (menuRect) {
      setMenuRect(null)
    } else if (rowRef.current) {
      const rect = rowRef.current.getBoundingClientRect()
      setMenuRect({ left: rect.left, top: rect.top, width: rect.width })
    }
  }, [isAuthenticated, menuRect, onOpenLogin])

  // 显示名称
  const displayName = resolveAccountDisplayName(
    accountInfo,
    profile.displayName || (isAuthenticated ? (lang === 'zh' ? '已登录账户' : 'Connected') : 'ABU Agent'),
  )

  // 用户头像 Profile
  const avatarProfile: ChatUserProfile = {
    ...profile,
    displayName: accountInfo?.displayName || accountInfo?.username || profile.displayName,
  }

  // 余额
  const balance = accountInfo ? formatAbuQuota(accountInfo.quota + accountInfo.temporaryQuota) : null

  return (
    <div className="px-2">
      <div
        ref={rowRef}
        className={`flex w-full items-center gap-1 rounded-lg px-1.5 py-1 transition-colors ${
          menuRect || settingsActive
            ? 'bg-black/[0.06] dark:bg-white/[0.1]'
            : 'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]'
        }`}
      >
        <button
          type="button"
          onClick={toggleMenu}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
          aria-haspopup="menu"
          aria-expanded={menuRect !== null}
          title={isAuthenticated ? (lang === 'zh' ? '点击查看账户详情' : 'Click to view account details') : (lang === 'zh' ? '点击登录' : 'Click to log in')}
        >
          {isAuthenticated ? (
            <UserAvatar profile={avatarProfile} size={22} />
          ) : (
            <div className="flex h-[22px] w-[22px] items-center justify-center rounded-full bg-neutral-200 dark:bg-neutral-700">
              <User size={12} strokeWidth={2} className="text-neutral-500 dark:text-neutral-400" />
            </div>
          )}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <span className="min-w-0 truncate text-[12.5px] font-medium text-neutral-700 dark:text-neutral-300">
                {displayName}
              </span>
              {accountInfo?.group === 'vip' && (
                <Crown size={12} className="shrink-0 text-yellow-500" />
              )}
            </div>
            {isAuthenticated && balance && (
              <div className="flex items-center gap-1 text-[10px] text-neutral-500 dark:text-neutral-400">
                <Coins size={10} />
                <span className="tabular-nums">${balance}</span>
              </div>
            )}
            {!isAuthenticated && (
              <div className="text-[10px] text-neutral-500 dark:text-neutral-400">
                {lang === 'zh' ? '未登录' : 'Not logged in'}
              </div>
            )}
          </div>
        </button>
        <IconButton
          size="xs"
          label={t.settings}
          onClick={() => {
            setMenuRect(null)
            onOpenSettings()
          }}
        >
          <Settings strokeWidth={1.75} />
        </IconButton>
      </div>

      {menuRect && isAuthenticated && (
        <UserAccountMenu
          triggerRect={menuRect}
          lang={lang}
          accountInfo={accountInfo}
          loading={loading}
          checkinStats={checkinStats}
          checkinLoading={checkinLoading}
          checkinError={checkinError}
          checkinReward={checkinReward}
          onCheckin={() => void handleCheckin()}
          onRecharge={openTopup}
          onOpenSettings={onOpenSettings}
          onLogout={onLogout}
          onClose={() => setMenuRect(null)}
        />
      )}
    </div>
  )
})
