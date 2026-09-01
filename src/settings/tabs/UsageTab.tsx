import { useEffect, useState } from 'react'
import { RefreshCw, TrendingUp, DollarSign, Clock, Activity } from 'lucide-react'
import { Button } from '../../components/Button'
import { SettingsGroup } from '../components'
import * as abuApi from '../../api/abuApi'
import type { AgentTask } from '../../api/abuApi'

interface UsageStats {
  total_tasks: number
  total_tokens: number
  total_quota: number
  tasks: AgentTask[]
}

interface I18n {
  [key: string]: string
}

function formatDate(timestamp: string, lang: 'zh' | 'en'): string {
  const date = new Date(timestamp)
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
  // 假设 quota 单位是分（100分 = 1元）
  const yuan = (quota / 100).toFixed(2)
  return lang === 'zh' ? `¥${yuan}` : `$${yuan}`
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
  t,
  lang,
  settings,
}: {
  t: I18n
  lang: 'zh' | 'en'
  settings: any
}) {
  const [stats, setStats] = useState<UsageStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const isCloudMode = settings.runtime_mode === 'cloud'

  useEffect(() => {
    if (isCloudMode) {
      loadUsageStats()
    } else {
      setLoading(false)
    }
  }, [isCloudMode])

  const loadUsageStats = async () => {
    try {
      setLoading(true)
      const tasks = await abuApi.listTasks()

      // 计算统计信息
      const totalTokens = tasks.reduce((sum, t) => sum + (t.total_tokens || 0), 0)
      const totalQuota = tasks.reduce((sum, t) => sum + (t.consumed_quota || 0), 0)

      setStats({
        total_tasks: tasks.length,
        total_tokens: totalTokens,
        total_quota: totalQuota,
        tasks: tasks
      })
    } catch (error) {
      console.error('Failed to load usage stats:', error)
      setStats(null)
    } finally {
      setLoading(false)
    }
  }

  const handleRefresh = async () => {
    setRefreshing(true)
    await loadUsageStats()
    setRefreshing(false)
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
        description={lang === 'zh' ? '查看您的任务使用情况' : 'View your task usage'}
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
                  <Activity size={14} />
                  {lang === 'zh' ? '总 Token 数' : 'Total Tokens'}
                </div>
                <div className="text-2xl font-bold text-neutral-900 dark:text-neutral-100">
                  {stats.total_tokens.toLocaleString()}
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
                    key={task.task_id}
                    className="p-3 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-800/50 hover:border-neutral-300 dark:hover:border-neutral-600 transition-colors"
                  >
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
                          {lang === 'zh' ? 'Tokens：' : 'Tokens: '}
                        </span>
                        <span className="font-medium text-neutral-700 dark:text-neutral-300">
                          {task.total_tokens.toLocaleString()}
                        </span>
                      </div>
                      <div>
                        <span className="text-neutral-500 dark:text-neutral-400">
                          {lang === 'zh' ? '提示：' : 'Prompt: '}
                        </span>
                        <span className="font-medium text-neutral-700 dark:text-neutral-300">
                          {task.prompt_tokens.toLocaleString()}
                        </span>
                      </div>
                      <div>
                        <span className="text-neutral-500 dark:text-neutral-400">
                          {lang === 'zh' ? '完成：' : 'Completion: '}
                        </span>
                        <span className="font-medium text-neutral-700 dark:text-neutral-300">
                          {task.completion_tokens.toLocaleString()}
                        </span>
                      </div>
                    </div>
                    <div className="mt-2 pt-2 border-t border-neutral-100 dark:border-neutral-700 text-xs">
                      <span className="text-neutral-500 dark:text-neutral-400">
                        {lang === 'zh' ? '消费：' : 'Cost: '}
                      </span>
                      <span className="font-semibold text-neutral-900 dark:text-neutral-100">
                        {formatQuota(task.consumed_quota, lang)}
                      </span>
                    </div>
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
