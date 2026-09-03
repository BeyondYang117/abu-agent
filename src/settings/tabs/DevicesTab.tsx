import { useCallback, useEffect, useState } from 'react'
import { Trash2, RefreshCw, CheckCircle } from 'lucide-react'
import { Button } from '../../components/Button'
import { SettingsGroup } from '../components'
import * as abuApi from '../../api/abuApi'
import type { AgentDevice } from '../../api/abuApi'
import { useAbuApiAuth } from '../../api/abuApiAuth'

// 使用 API 返回的设备类型
type Device = AgentDevice

function formatLastSeen(timestamp: number, lang: 'zh' | 'en'): string {
  const date = new Date(timestamp * 1000) // Unix timestamp to milliseconds
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  const minutes = Math.floor(diff / 60000)
  const hours = Math.floor(diff / 3600000)
  const days = Math.floor(diff / 86400000)

  if (lang === 'zh') {
    if (minutes < 1) return '刚刚'
    if (minutes < 60) return `${minutes} 分钟前`
    if (hours < 24) return `${hours} 小时前`
    if (days < 30) return `${days} 天前`
    return date.toLocaleDateString('zh-CN')
  } else {
    if (minutes < 1) return 'Just now'
    if (minutes < 60) return `${minutes}m ago`
    if (hours < 24) return `${hours}h ago`
    if (days < 30) return `${days}d ago`
    return date.toLocaleDateString('en-US')
  }
}

export function DevicesTab({
  lang,
}: {
  lang: 'zh' | 'en'
}) {
  const [devices, setDevices] = useState<Device[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [revoking, setRevoking] = useState<string | null>(null)
  const { deviceId: currentDeviceId } = useAbuApiAuth()

  const loadDevices = useCallback(async () => {
    try {
      setLoading(true)
      setError(null)
      const deviceList = await abuApi.listDevices()
      setDevices(deviceList)
    } catch (error) {
      console.error('Failed to load devices:', error)
      setDevices([])
      setError(error instanceof Error ? error.message : (lang === 'zh' ? '无法加载设备列表' : 'Failed to load devices'))
    } finally {
      setLoading(false)
    }
  }, [lang])

  useEffect(() => {
    void loadDevices()
  }, [loadDevices])

  const handleRefresh = async () => {
    setRefreshing(true)
    await loadDevices()
    setRefreshing(false)
  }

  const handleRevoke = async (deviceId: string) => {
    if (!confirm(lang === 'zh' ? '确定要注销此设备吗？' : 'Revoke this device?')) {
      return
    }

    try {
      setRevoking(deviceId)
      await abuApi.revokeDevice(deviceId)
      await loadDevices()
    } catch (error) {
      console.error('Failed to revoke device:', error)
      alert(lang === 'zh' ? '注销设备失败' : 'Failed to revoke device')
    } finally {
      setRevoking(null)
    }
  }

  return (
    <div className="flex-1 overflow-y-auto">
      <SettingsGroup title={lang === 'zh' ? '设备管理' : 'Device Management'}>
        <div className="mb-2 text-sm text-neutral-600 dark:text-neutral-400">
          {lang === 'zh' ? '管理已登录的设备' : 'Manage logged-in devices'}
        </div>
        <div className="flex justify-end mb-4">
          <Button
            size="sm"
            variant="ghost"
            onClick={handleRefresh}
            disabled={refreshing}
            data-tauri-drag-region="false"
          >
            <RefreshCw size={14} className={refreshing ? 'animate-spin' : ''} />
            {lang === 'zh' ? '刷新' : 'Refresh'}
          </Button>
        </div>

        {loading ? (
          <div className="text-center py-8 text-sm text-neutral-500 dark:text-neutral-400">
            {lang === 'zh' ? '加载中...' : 'Loading...'}
          </div>
        ) : error ? (
          <div className="text-center py-8 text-sm text-red-600 dark:text-red-400">
            {error}
          </div>
        ) : devices.length === 0 ? (
          <div className="text-center py-8 text-sm text-neutral-500 dark:text-neutral-400">
            {lang === 'zh' ? '没有已登录的设备' : 'No devices found'}
          </div>
        ) : (
          <div className="space-y-3">
            {devices.map((device) => {
              const isCurrent = device.id === currentDeviceId

              return (
                <div
                  key={device.id}
                  className={`
                    flex items-center justify-between p-4 rounded-lg border
                    ${isCurrent
                      ? 'border-blue-300 dark:border-blue-700 bg-blue-50 dark:bg-blue-900/20'
                      : 'border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-800/50'
                    }
                  `}
                >
                  <div className="flex items-center gap-3 flex-1">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-medium text-neutral-900 dark:text-neutral-100 truncate">
                          {device.device_name}
                        </span>
                        {isCurrent && (
                          <span className="flex items-center gap-1 text-xs text-blue-600 dark:text-blue-400 whitespace-nowrap">
                            <CheckCircle size={12} />
                            {lang === 'zh' ? '当前设备' : 'Current'}
                          </span>
                        )}
                      </div>
                      <div className="text-xs text-neutral-600 dark:text-neutral-400 mt-1">
                        <span className="capitalize">{device.platform}</span>
                        {' • '}
                        <span>{formatLastSeen(device.last_seen_at, lang)}</span>
                      </div>
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => handleRevoke(device.id)}
                    disabled={isCurrent || revoking === device.id}
                    data-tauri-drag-region="false"
                    className="text-red-600 hover:text-red-700 hover:bg-red-50 dark:text-red-400 dark:hover:text-red-300 dark:hover:bg-red-900/20"
                  >
                    <Trash2 size={14} />
                    {lang === 'zh' ? '注销' : 'Revoke'}
                  </Button>
                </div>
              )
            })}
          </div>
        )}
      </SettingsGroup>
    </div>
  )
}
