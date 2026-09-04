import { useCallback, useEffect, useState } from 'react'
import { RefreshCw, DollarSign, Activity } from 'lucide-react'
import { Button } from '../../components/Button'
import { SettingsGroup } from '../components'
import * as abuApi from '../../api/abuApi'
import type { AgentEntitlement } from '../../api/abuApi'
import { formatAbuQuota } from '../../api/quota'
import type { Settings } from '../../api/tauri'

export function UsageTab({ lang, settings }: { lang: 'zh' | 'en'; settings: Pick<Settings, 'runtimeMode'> }) {
  const [account, setAccount] = useState<{ quota: number; used_quota: number } | null>(null)
  const [entitlements, setEntitlements] = useState<AgentEntitlement[]>([])
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const isCloudMode = settings.runtimeMode?.trim().toLowerCase() === 'cloud'

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [info, plans] = await Promise.all([abuApi.getAbuApiClient().getUserInfo(), abuApi.listEntitlements()])
      setAccount({ quota: info.quota, used_quota: info.used_quota })
      setEntitlements(plans)
    } catch (err) {
      setAccount(null)
      setEntitlements([])
      setError(err instanceof Error ? err.message : (lang === 'zh' ? '无法加载账户用量' : 'Failed to load account usage'))
    } finally {
      setLoading(false)
    }
  }, [lang])

  useEffect(() => { if (isCloudMode) void load(); else setLoading(false) }, [isCloudMode, load])

  if (!isCloudMode) {
    return <SettingsGroup title={lang === 'zh' ? '使用统计' : 'Usage Statistics'}><div className="py-10 text-center text-sm text-neutral-500"><Activity size={40} className="mx-auto mb-3 opacity-50" />{lang === 'zh' ? '使用统计仅在云端模式下可用' : 'Usage statistics are only available in Cloud mode'}</div></SettingsGroup>
  }

  return <SettingsGroup title={lang === 'zh' ? '账户用量' : 'Account Usage'}>
    <div className="flex justify-end mb-4"><Button size="sm" variant="ghost" onClick={async () => { setRefreshing(true); await load(); setRefreshing(false) }} disabled={loading || refreshing}><RefreshCw size={14} className={refreshing ? 'animate-spin' : ''} />{lang === 'zh' ? '刷新' : 'Refresh'}</Button></div>
    {loading ? <div className="py-8 text-center text-sm text-neutral-500">{lang === 'zh' ? '加载中...' : 'Loading...'}</div> : error ? <div className="py-8 text-center text-sm text-red-600">{error}</div> : account ? <>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
        <div className="p-4 rounded-lg border border-neutral-200 dark:border-neutral-700"><div className="flex items-center gap-2 text-xs text-neutral-500 mb-2"><DollarSign size={14} />{lang === 'zh' ? '已使用额度' : 'Used quota'}</div><div className="text-2xl font-bold">${formatAbuQuota(account.used_quota)}</div></div>
        <div className="p-4 rounded-lg border border-neutral-200 dark:border-neutral-700"><div className="flex items-center gap-2 text-xs text-neutral-500 mb-2"><DollarSign size={14} />{lang === 'zh' ? '账户额度' : 'Account quota'}</div><div className="text-2xl font-bold">${formatAbuQuota(account.quota)}</div></div>
      </div>
      <h3 className="text-sm font-medium mb-3">{lang === 'zh' ? '套餐权益' : 'Plan entitlements'}</h3>
      {entitlements.length === 0 ? <div className="py-6 text-center text-sm text-neutral-500">{lang === 'zh' ? '暂无有效套餐' : 'No active plans'}</div> : <div className="space-y-2">{entitlements.map((item) => <div key={item.id} className="p-3 rounded-lg border border-neutral-200 dark:border-neutral-700"><div className="flex justify-between gap-2"><span className="font-medium text-sm">{item.plan_name || `#${item.plan_id}`}</span><span className="text-xs text-neutral-500">{item.bound_groups.join(', ')}</span></div><div className="mt-2 text-xs text-neutral-600">{item.monthly_limit_usd != null ? `${lang === 'zh' ? '月度用量' : 'Monthly usage'}: $${item.monthly_usage_usd.toFixed(2)} / $${item.monthly_limit_usd.toFixed(2)}` : item.weekly_limit_usd != null ? `${lang === 'zh' ? '周用量' : 'Weekly usage'}: $${item.weekly_usage_usd.toFixed(2)} / $${item.weekly_limit_usd.toFixed(2)}` : item.daily_limit_usd != null ? `${lang === 'zh' ? '日用量' : 'Daily usage'}: $${item.daily_usage_usd.toFixed(2)} / $${item.daily_limit_usd.toFixed(2)}` : (lang === 'zh' ? '周期额度不限' : 'No window limit')}</div></div>)}</div>}
    </> : null}
  </SettingsGroup>
}
