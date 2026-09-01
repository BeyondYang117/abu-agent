import { useEffect, useState } from 'react'
import { LogOut, User, Mail, Coins } from 'lucide-react'
import { Button } from '../../components/Button'
import { SettingsGroup } from '../components'
import { abuApiAuthStore } from '../../api/abuApiAuth'
import * as api from '../../api/tauri'

interface AccountInfo {
  username?: string
  email?: string
  quota?: number
  quotaUnit?: string
}

export function AccountInfoCard({
  t,
  lang,
  onLogout,
}: {
  t: any
  lang: 'zh' | 'en'
  onLogout: () => void
}) {
  const [accountInfo, setAccountInfo] = useState<AccountInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const isAuthenticated = abuApiAuthStore((state) => state.isAuthenticated)

  useEffect(() => {
    if (isAuthenticated) {
      loadAccountInfo()
    } else {
      setLoading(false)
    }
  }, [isAuthenticated])

  const loadAccountInfo = async () => {
    try {
      setLoading(true)
      // TODO: 实现获取账户信息的 API 调用
      // const info = await abuApi.getUserInfo()
      // setAccountInfo(info)

      // 临时：从存储的配置中获取基本信息
      const config = await api.loadAbuApiConfig()
      setAccountInfo({
        username: 'User', // 需要从 API 获取
        email: config.session_token ? 'user@example.com' : undefined,
        quota: undefined,
        quotaUnit: '元'
      })
    } catch (error) {
      console.error('Failed to load account info:', error)
      setAccountInfo(null)
    } finally {
      setLoading(false)
    }
  }

  if (!isAuthenticated) {
    return null
  }

  return (
    <SettingsGroup title={lang === 'zh' ? '账户信息' : 'Account'}>
      <div className="space-y-3 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-neutral-50 dark:bg-neutral-800/50 p-4">
        {loading ? (
          <div className="text-sm text-neutral-500 dark:text-neutral-400">
            {lang === 'zh' ? '加载中...' : 'Loading...'}
          </div>
        ) : accountInfo ? (
          <>
            <div className="flex items-center gap-2 text-sm">
              <User size={14} className="text-neutral-500 dark:text-neutral-400" />
              <span className="font-medium text-neutral-900 dark:text-neutral-100">
                {accountInfo.username || (lang === 'zh' ? '未知用户' : 'Unknown User')}
              </span>
            </div>
            {accountInfo.email && (
              <div className="flex items-center gap-2 text-sm">
                <Mail size={14} className="text-neutral-500 dark:text-neutral-400" />
                <span className="text-neutral-700 dark:text-neutral-300">
                  {accountInfo.email}
                </span>
              </div>
            )}
            {accountInfo.quota !== undefined && (
              <div className="flex items-center gap-2 text-sm">
                <Coins size={14} className="text-neutral-500 dark:text-neutral-400" />
                <span className="text-neutral-700 dark:text-neutral-300">
                  {lang === 'zh' ? '余额：' : 'Balance: '}
                  <span className="font-semibold text-neutral-900 dark:text-neutral-100">
                    {accountInfo.quota} {accountInfo.quotaUnit}
                  </span>
                </span>
              </div>
            )}
            <div className="pt-2 border-t border-neutral-200 dark:border-neutral-700">
              <Button
                size="sm"
                variant="ghost"
                onClick={onLogout}
                data-tauri-drag-region="false"
                className="w-full"
              >
                <LogOut size={14} />
                {lang === 'zh' ? '退出登录' : 'Log Out'}
              </Button>
            </div>
          </>
        ) : (
          <div className="text-sm text-neutral-500 dark:text-neutral-400">
            {lang === 'zh' ? '无法加载账户信息' : 'Failed to load account info'}
          </div>
        )}
      </div>
    </SettingsGroup>
  )
}
