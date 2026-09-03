import { useCallback, useEffect, useState } from 'react'
import { RefreshCw, TrendingUp, DollarSign, Clock, Activity } from 'lucide-react'
import { Button } from '../../components/Button'
import { SettingsGroup } from '../components'
import * as abuApi from '../../api/abuApi'
import type { AgentEntitlement, AgentTask, AgentTaskAttempt } from '../../api/abuApi'
import { formatAbuQuota } from '../../api/quota'
import type { Settings } from '../../api/tauri'

interface UsageStats {
  total_tasks: number
  total_quota: number
  tasks: AgentTask[]
  entitlements: AgentEntitlement[]
}

function formatDate(timestamp: number, lang: 'zh' | 'en'): string {
  const date = new Date(timestamp > 10_000_000_000 ? timestamp : timestamp * 1000)
  if (lang === 'zh') {
    return date.toLocaleDateString('zh-CN', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    })
  } else {
    return date.toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    })
  }
}

function formatQuota(quota: number, lang: 'zh' | 'en'): string {
  const amount = formatAbuQuota(quota)
  return lang === 'zh' ? `$${amount}` : `$${amount}`
}

function getTaskStatusColor(status: string): string {
  switch (status) {
    case 'succeeded':
      return 'text-green-600 dark:text-green-400 bg-green-50 dark:bg-green-900/20'
    case 'failed':
      return 'text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20'
    case 'cancelled':
      return 'text-yellow-600 dark:text-yellow-400 bg-yellow-50 dark:bg-yellow-900/20'
    case 'running':
      return 'text-blue-600 dark:text-blue-400 bg-blue-50 dark:bg-blue-900/20'
    default:
      return 'text-neutral-600 dark:text-neutral-400 bg-neutral-50 dark:bg-neutral-800/50'
  }
}

function getTaskStatusLabel(status: string, lang: 'zh' | 'en'): string {
  if (lang === 'zh') {
    switch (status) {
      case 'succeeded': return '成功'
      case 'failed': return '失败'
      case 'cancelled': return '已取消'
      case 'running': return '运行中'
      default: return status
    }
  } else {
    return status.charAt(0).toUpperCase() + status.slice(1)
  }
}

export function UsageTab({
  lang,
  settings,
}: {
  lang: 'zh' | 'en'
  settings: Pick<Settings, 'runtimeMode'>
}) {
  const [stats, setStats] = useState<UsageStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [attempts, setAttempts] = useState<Record<string, AgentTaskAttempt[]>>({})
  const isCloudMode = settings.runtimeMode?.trim().toLowerCase() === 'cloud'
  const planNames = new Map(stats?.entitlements.map((item) => [item.id, item.plan_name]) ?? [])

  const loadUsageStats = useCallback(async () => {
    try {
      setLoading(true)
      setError(null)
      const [tasks, entitlements] = await Promise.all([
        abuApi.listTasks({ limit: 100 }),
        abuApi.listEntitlements(),
      ])

      // 计算统计信息
      const totalQuota = tasks.reduce((sum, t) => sum + (t.consumed_quota || 0), 0)

      setStats({
        total_tasks: tasks.length,
        total_quota: totalQuota,
        tasks,
        entitlements,
      })
    } catch (error) {
      console.error('Failed to load usage stats:', error)
      setStats(null)
      setError(error instanceof Error ? error.message : (lang === 'zh' ? '无法加载使用统计' : 'Failed to load usage statistics'))
    } finally {
      setLoading(false)
    }
  }, [lang])

  useEffect(() => {
    if (isCloudMode) {
      void loadUsageStats()
    } else {
      setLoading(false)
    }
  }, [isCloudMode, loadUsageStats])

  const handleRefresh = async () => {
    setRefreshing(true)
    await loadUsageStats()
    setRefreshing(false)
  }

  const loadAttempts = async (taskId: string) => {
    if (attempts[taskId]) return
    try {
      const rows = await abuApi.listTaskAttempts(taskId)
      setAttempts((current) => ({ ...current, [taskId]: rows }))
    } catch (error) {
      console.error('Failed to load task attempts:', error)
      setAttempts((current) => ({ ...current, [taskId]: [] }))
    }
  }

  if (!isCloudMode) {
    return (
      <div className="flex-1 overflow-y-auto">
        <SettingsGroup
          title={lang === 'zh' ? '使用统计' : 'Usage Statistics'}
        >
          <div className="text-center py-12 text-neutral-500 dark:text-neutral-400">
            <Activity size={48} className="mx-auto mb-4 opacity-50" />
            <p className="text-sm">
              {lang === 'zh'
                ? '使用统计仅在云端模式下可用'
                : 'Usage statistics are only available in Cloud mode'
              }
            </p>
          </div>
        </SettingsGroup>
      </div>
    )
  }

  return (
    <div className="flex-1 overflow-y-auto">
      <SettingsGroup
        title={lang === 'zh' ? '使用统计' : 'Usage Statistics'}
      >
        <div className="flex justify-end mb-4">
          <Button
            size="sm"
            variant="ghost"
            onClick={handleRefresh}
            disabled={refreshing || loading}
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
        ) : stats ? (
          <>
            {/* 统计卡片 */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
              <div className="p-4 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-800/50">
                <div className="flex items-center gap-2 text-neutral-500 dark:text-neutral-400 text-xs mb-2">
                  <TrendingUp size={14} />
                  {lang === 'zh' ? '总任务数' : 'Total Tasks'}
                </div>
                <div className="text-2xl font-bold text-neutral-900 dark:text-neutral-100">
                  {stats.total_tasks}
                </div>
              </div>

              <div className="p-4 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-800/50">
                <div className="flex items-center gap-2 text-neutral-500 dark:text-neutral-400 text-xs mb-2">
                  <DollarSign size={14} />
                  {lang === 'zh' ? '总消费' : 'Total Cost'}
                </div>
                <div className="text-2xl font-bold text-neutral-900 dark:text-neutral-100">
                  {formatQuota(stats.total_quota, lang)}
                </div>
              </div>
            </div>

            {/* 套餐权益 */}
            {stats.entitlements.length > 0 && (
              <div className="mb-6">
                <h3 className="text-sm font-medium text-neutral-700 dark:text-neutral-300 mb-3">
                  {lang === 'zh' ? '套餐权益' : 'Plan entitlements'}
                </h3>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {stats.entitlements.map((entitlement) => {
                    const limit = entitlement.monthly_limit_usd ?? entitlement.weekly_limit_usd ?? entitlement.daily_limit_usd
                    const usage = entitlement.monthly_limit_usd != null
                      ? entitlement.monthly_usage_usd
                      : entitlement.weekly_limit_usd != null
                        ? entitlement.weekly_usage_usd
                        : entitlement.daily_usage_usd
                    const percentage = limit && limit > 0 ? Math.min(usage / limit * 100, 100) : 0
                    return (
                      <div key={entitlement.id} className="p-3 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-800/50">
                        <div className="flex items-center justify-between gap-2">
                          <span className="font-medium text-sm text-neutral-800 dark:text-neutral-200">
                            {entitlement.plan_name || `#${entitlement.plan_id}`}
                          </span>
                          <span className="text-xs text-neutral-500 dark:text-neutral-400">
                            {entitlement.bound_groups.join(', ') || (lang === 'zh' ? '默认分组' : 'Default group')}
                          </span>
                        </div>
                        <div className="mt-2 text-xs text-neutral-600 dark:text-neutral-400">
                          {limit && limit > 0
                            ? `${lang === 'zh' ? '周期用量' : 'Window usage'}: $${usage.toFixed(2)} / $${limit.toFixed(2)}`
                            : (lang === 'zh' ? '周期额度不限' : 'No window limit')}
                        </div>
                        {limit && limit > 0 && (
                          <div className="mt-2 h-1 rounded-full bg-neutral-200 dark:bg-neutral-700 overflow-hidden">
                            <div className="h-full rounded-full bg-emerald-500" style={{ width: `${percentage}%` }} />
                          </div>
                        )}
                        {entitlement.extra_quota_remaining_usd > 0 && (
                          <div className="mt-2 text-xs text-emerald-600 dark:text-emerald-400">
                            {lang === 'zh' ? '增量包剩余' : 'Extra quota'}: ${entitlement.extra_quota_remaining_usd.toFixed(2)}
                          </div>
                        )}
                      </div>
                    )
                  })}
                </div>
              </div>
            )}

            {/* 任务列表 */}
            <div className="space-y-2">
              <h3 className="text-sm font-medium text-neutral-700 dark:text-neutral-300 mb-3">
                {lang === 'zh' ? '最近任务' : 'Recent Tasks'}
              </h3>
              {stats.tasks.length === 0 ? (
                <div className="text-center py-8 text-sm text-neutral-500 dark:text-neutral-400">
                  {lang === 'zh' ? '暂无任务记录' : 'No tasks yet'}
                </div>
              ) : (
                stats.tasks.slice(0, 20).map((task) => (
                  <div
                    key={task.id}
                    className="p-3 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-800/50 hover:border-neutral-300 dark:hover:border-neutral-600 transition-colors"
                  >
                    <button
                      type="button"
                      className="float-right text-xs text-blue-600 dark:text-blue-400 hover:underline"
                      onClick={() => void loadAttempts(task.id)}
                    >
                      {lang === 'zh' ? '查看执行轨迹' : 'View execution trace'}
                    </button>
                    <div className="flex items-start justify-between mb-2">
                      <div className="flex items-center gap-2">
                        <span className={`text-xs px-2 py-0.5 rounded-full ${getTaskStatusColor(task.status)}`}>
                          {getTaskStatusLabel(task.status, lang)}
                        </span>
                        <span className="text-xs text-neutral-500 dark:text-neutral-400">
                          {task.type || 'chat'}
                        </span>
                      </div>
                      <div className="flex items-center gap-1 text-xs text-neutral-500 dark:text-neutral-400">
                        <Clock size={12} />
                        {formatDate(task.created_at, lang)}
                      </div>
                    </div>
                    <div className="grid grid-cols-3 gap-4 text-xs">
                      <div>
                        <span className="text-neutral-500 dark:text-neutral-400">
                          {lang === 'zh' ? '累计额度：' : 'Accumulated quota: '}
                        </span>
                        <span className="font-medium text-neutral-700 dark:text-neutral-300">
                          {formatQuota(task.consumed_quota, lang)}
                        </span>
                      </div>
                      <div>
                        <span className="text-neutral-500 dark:text-neutral-400">
                          {lang === 'zh' ? '累计软上限：' : 'Soft cap: '}
                        </span>
                        <span className="font-medium text-neutral-700 dark:text-neutral-300">
                          {task.soft_cap > 0 ? formatQuota(task.soft_cap, lang) : (lang === 'zh' ? '不限' : 'Unlimited')}
                        </span>
                      </div>
                      <div>
                        <span className="text-neutral-500 dark:text-neutral-400">
                          {lang === 'zh' ? '累计硬上限：' : 'Hard cap: '}
                        </span>
                        <span className="font-medium text-neutral-700 dark:text-neutral-300">
                          {task.hard_cap > 0 ? formatQuota(task.hard_cap, lang) : (lang === 'zh' ? '不限' : 'Unlimited')}
                        </span>
                      </div>
                    </div>
                    {(task.billing_group || task.requested_model || task.subscription_id) && (
                      <div className="mt-2 pt-2 border-t border-neutral-100 dark:border-neutral-700 text-xs text-neutral-600 dark:text-neutral-400">
                        <div className="flex flex-wrap gap-x-4 gap-y-1">
                          {task.requested_model && (
                            <span>
                              {lang === 'zh' ? '模型：' : 'Model: '}
                              <strong className="font-medium text-neutral-800 dark:text-neutral-200">{task.requested_model}</strong>
                            </span>
                          )}
                          {task.billing_group && (
                            <span>
                              {lang === 'zh' ? '计费分组：' : 'Billing group: '}
                              <strong className="font-medium text-neutral-800 dark:text-neutral-200">{task.billing_group}</strong>
                            </span>
                          )}
                          {task.subscription_id && (
                            <span>
                              {lang === 'zh' ? '套餐：' : 'Plan: '}
                              <strong className="font-medium text-emerald-700 dark:text-emerald-400">
                                {planNames.get(task.subscription_id) || `#${task.subscription_id}`}
                              </strong>
                            </span>
                          )}
                          {task.selected_channel_id ? (
                            <span>
                              {lang === 'zh' ? '渠道：' : 'Channel: '}
                              #{task.selected_channel_id}
                            </span>
                          ) : null}
                        </div>
                      </div>
                    )}
                    <div className="mt-2 pt-2 border-t border-neutral-100 dark:border-neutral-700 text-xs">
                      <span className="text-neutral-500 dark:text-neutral-400">
                        {lang === 'zh' ? '消费：' : 'Cost: '}
                      </span>
                      <span className="font-semibold text-neutral-900 dark:text-neutral-100">
                        {formatQuota(task.consumed_quota, lang)}
                      </span>
                    </div>
                    {attempts[task.id]?.length ? (
                      <div className="mt-3 space-y-1.5 rounded bg-neutral-50 dark:bg-neutral-900/40 p-2 text-xs">
                        {attempts[task.id].map((attempt) => (
                          <div key={attempt.id} className="flex flex-wrap gap-x-3 gap-y-1 text-neutral-600 dark:text-neutral-400">
                            <span>{attempt.model || task.requested_model || '-'}</span>
                            <span>{attempt.billing_group || task.billing_group || '-'}</span>
                            <span>{attempt.channel_id ? `#${attempt.channel_id}` : '-'}</span>
                            <span>{formatQuota(attempt.quota, lang)}</span>
                            <span className="text-neutral-400">{attempt.status}</span>
                          </div>
                        ))}
                      </div>
                    ) : null}
                  </div>
                ))
              )}
            </div>
          </>
        ) : (
          <div className="text-center py-8 text-sm text-neutral-500 dark:text-neutral-400">
            {lang === 'zh' ? '无法加载使用统计' : 'Failed to load usage statistics'}
          </div>
        )}
      </SettingsGroup>
    </div>
  )
}
