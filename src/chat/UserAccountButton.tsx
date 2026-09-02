import { memo, useCallback, useEffect, useRef, useState } from 'react'
import { LogIn, LogOut, User, Settings, Coins, Crown, Calendar } from 'lucide-react'
import { useAbuApiAuth } from '../api/abuApiAuth'
import { getAbuApiClient } from '../api/abuApi'
import { UserAvatar } from './UserAvatar'
import type { ChatUserProfile } from './types'
import { i18n, type Lang } from '../settings/i18n'
import { IconButton } from '../components/Button'
import { UserAccountMenu } from './UserAccountMenu'

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
  const [menuRect, setMenuRect] = useState<{ left: number; top: number; width: number } | null>(null)
  const rowRef = useRef<HTMLDivElement>(null)

  // 加载用户信息
  useEffect(() => {
    let cancelled = false

    if (isAuthenticated) {
      setLoading(true)
      getAbuApiClient()
        .getUserInfo()
        .then((info) => {
          if (!cancelled) {
            setAccountInfo({
              username: info.username,
              displayName: info.display_name,
              email: info.email,
              quota: info.quota,
              usedQuota: info.used_quota,
              group: info.group,
            })
          }
        })
        .catch((err) => {
          console.error('Failed to load account info:', err)
          if (!cancelled) {
            setAccountInfo(null)
          }
        })
        .finally(() => {
          if (!cancelled) setLoading(false)
        })
    } else {
      setAccountInfo(null)
    }

    return () => {
      cancelled = true
    }
  }, [isAuthenticated])

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
  const displayName = accountInfo?.displayName || accountInfo?.username || profile.displayName || 'ABU Agent'

  // 余额
  const balance = accountInfo ? (accountInfo.quota / 100).toFixed(2) : null

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
          title={isAuthenticated ? lang === 'zh' ? '点击查看账户详情' : 'Click to view account details' : lang === 'zh' ? '点击登录' : 'Click to log in'}
        >
          {isAuthenticated ? (
            <UserAvatar profile={profile} size={22} />
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
          onOpenSettings={onOpenSettings}
          onLogout={onLogout}
          onClose={() => setMenuRect(null)}
        />
      )}
    </div>
  )
})
