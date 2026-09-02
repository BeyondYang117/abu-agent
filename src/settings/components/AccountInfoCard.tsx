import { useEffect, useState } from 'react'
import { LogOut, User, Mail, Coins, RefreshCw, LogIn } from 'lucide-react'
import { Button } from '../../components/Button'
import { SettingsGroup } from '../components'
import { useAbuApiAuth } from '../../api/abuApiAuth'
import { getAbuApiClient } from '../../api/abuApi'

interface AccountInfo {
  username: string
  displayName?: string
  email?: string
  quota: number
  usedQuota: number
  group: string
}

export function AccountInfoCard({
  lang,
  onLogout,
  onLoginRequest,
}: {
  lang: 'zh' | 'en'
  onLogout: () => void
  onLoginRequest?: () => void
}) {
  const [accountInfo, setAccountInfo] = useState<AccountInfo | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const { isAuthenticated } = useAbuApiAuth()

  useEffect(() => {
    if (isAuthenticated) {
      loadAccountInfo()
    }
  }, [isAuthenticated])

  const loadAccountInfo = async () => {
    try {
      setLoading(true)
      setError(null)

      const client = getAbuApiClient()
      if (!client) {
        throw new Error('ABU API client not initialized')
      }

      const info = await client.getUserInfo()
      setAccountInfo({
        username: info.username,
        displayName: info.display_name,
        email: info.email,
        quota: info.quota,
        usedQuota: info.used_quota,
        group: info.group,
      })
    } catch (err) {
      console.error('Failed to load account info:', err)
      setError(err instanceof Error ? err.message : String(err))
      setAccountInfo(null)
    } finally {
      setLoading(false)
    }
  }

  if (!isAuthenticated) {
    return (
      <SettingsGroup title={lang === 'zh' ? '账户信息' : 'Account'}>
        <div className="space-y-3 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-neutral-50 dark:bg-neutral-800/50 p-4">
          <div className="text-sm text-neutral-500 dark:text-neutral-400 text-center py-2">
            {lang === 'zh' ? '未登录 ABU API 账户' : 'Not logged in to ABU API'}
          </div>
          {onLoginRequest && (
            <Button
              size="sm"
              variant="primary"
              onClick={onLoginRequest}
              data-tauri-drag-region="false"
              className="w-full"
            >
              <LogIn size={14} />
              {lang === 'zh' ? '登录账户' : 'Log In'}
            </Button>
          )}
        </div>
      </SettingsGroup>
    )
  }

  return (
    <SettingsGroup title={lang === 'zh' ? '账户信息' : 'Account'}>
      <div className="space-y-3 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-neutral-50 dark:bg-neutral-800/50 p-4">
        {loading ? (
          <div className="flex items-center justify-center gap-2 text-sm text-neutral-500 dark:text-neutral-400 py-2">
            <RefreshCw size={14} className="animate-spin" />
            {lang === 'zh' ? '加载中...' : 'Loading...'}
          </div>
        ) : error ? (
          <div className="space-y-3">
            <div className="text-sm text-red-600 dark:text-red-400">
              {lang === 'zh' ? '加载失败：' : 'Failed to load: '}
              {error}
            </div>
            {error.includes('401') || error.includes('403') || error.includes('Unauthorized') || error.includes('session') ? (
              <div className="text-xs text-neutral-600 dark:text-neutral-400 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded p-2">
                {lang === 'zh'
                  ? '登录已过期，请退出后重新登录'
                  : 'Session expired, please log out and log in again'}
              </div>
            ) : null}
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="ghost"
                onClick={loadAccountInfo}
                data-tauri-drag-region="false"
                className="flex-1"
              >
                <RefreshCw size={14} />
                {lang === 'zh' ? '重试' : 'Retry'}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={onLogout}
                data-tauri-drag-region="false"
                className="flex-1"
              >
                <LogOut size={14} />
                {lang === 'zh' ? '退出登录' : 'Log Out'}
              </Button>
            </div>
          </div>
        ) : accountInfo ? (
          <>
            <div className="flex items-center gap-2 text-sm">
              <User size={14} className="text-neutral-500 dark:text-neutral-400" />
              <span className="font-medium text-neutral-900 dark:text-neutral-100">
                {accountInfo.displayName || accountInfo.username}
              </span>
              {accountInfo.displayName && accountInfo.displayName !== accountInfo.username && (
                <span className="text-xs text-neutral-500 dark:text-neutral-400">
                  @{accountInfo.username}
                </span>
              )}
            </div>
            {accountInfo.email && (
              <div className="flex items-center gap-2 text-sm">
                <Mail size={14} className="text-neutral-500 dark:text-neutral-400" />
                <span className="text-neutral-700 dark:text-neutral-300">
                  {accountInfo.email}
                </span>
              </div>
            )}
            <div className="flex items-center gap-2 text-sm">
              <Coins size={14} className="text-neutral-500 dark:text-neutral-400" />
              <span className="text-neutral-700 dark:text-neutral-300">
                {lang === 'zh' ? '余额：' : 'Balance: '}
                <span className="font-semibold text-neutral-900 dark:text-neutral-100">
                  ${(accountInfo.quota / 100).toFixed(2)}
                </span>
                <span className="text-xs text-neutral-500 dark:text-neutral-400 ml-1">
                  ({lang === 'zh' ? '已用' : 'used'} ${(accountInfo.usedQuota / 100).toFixed(2)})
                </span>
              </span>
            </div>
            <div className="pt-2 border-t border-neutral-200 dark:border-neutral-700 flex gap-2">
              <Button
                size="sm"
                variant="ghost"
                onClick={loadAccountInfo}
                data-tauri-drag-region="false"
                className="flex-1"
              >
                <RefreshCw size={14} />
                {lang === 'zh' ? '刷新' : 'Refresh'}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={onLogout}
                data-tauri-drag-region="false"
                className="flex-1"
              >
                <LogOut size={14} />
                {lang === 'zh' ? '退出登录' : 'Log Out'}
              </Button>
            </div>
          </>
        ) : (
          <div className="text-sm text-neutral-500 dark:text-neutral-400 text-center py-2">
            {lang === 'zh' ? '无法加载账户信息' : 'Failed to load account info'}
          </div>
        )}
      </div>
    </SettingsGroup>
  )
}
