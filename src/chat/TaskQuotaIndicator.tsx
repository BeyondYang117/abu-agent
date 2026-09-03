import { AlertCircle, TrendingUp } from 'lucide-react'
import { useTaskUsage } from './useTaskUsage'
import { formatAbuQuota } from '../api/quota'

interface TaskQuotaIndicatorProps {
  taskId: string | null
  softCap?: number
  hardCap?: number
  lang: 'zh' | 'en'
  className?: string
}

interface UsageLevel {
  level: 'safe' | 'warning' | 'danger'
  color: string
  bgColor: string
  icon: React.ReactNode
}

function getUsageLevel(
  consumedQuota: number,
  softCap: number,
  hardCap: number
): UsageLevel {
  // A zero cap means the server has disabled that guard.
  const hasHardCap = hardCap > 0
  const hasSoftCap = softCap > 0

  if (hasHardCap && consumedQuota >= hardCap) {
    return {
      level: 'danger',
      color: 'text-red-600 dark:text-red-400',
      bgColor: 'bg-red-50 dark:bg-red-900/20 border-red-300 dark:border-red-700',
      icon: <AlertCircle size={14} />
    }
  } else if (hasSoftCap && consumedQuota >= softCap) {
    return {
      level: 'warning',
      color: 'text-yellow-600 dark:text-yellow-400',
      bgColor: 'bg-yellow-50 dark:bg-yellow-900/20 border-yellow-300 dark:border-yellow-700',
      icon: <AlertCircle size={14} />
    }
  } else {
    return {
      level: 'safe',
      color: 'text-neutral-600 dark:text-neutral-400',
      bgColor: 'bg-neutral-50 dark:bg-neutral-800/50 border-neutral-200 dark:border-neutral-700',
      icon: <TrendingUp size={14} />
    }
  }
}

function formatQuota(quota: number, lang: 'zh' | 'en'): string {
  const amount = formatAbuQuota(quota)
  return lang === 'zh' ? `$${amount}` : `$${amount}`
}

export function TaskQuotaIndicator({
  taskId,
  softCap,
  hardCap,
  lang,
  className = ''
}: TaskQuotaIndicatorProps) {
  const { usage, loading, error } = useTaskUsage(taskId)

  // 如果没有 taskId 或处于本地模式，不显示
  if (!taskId) {
    return null
  }

  // 加载中显示骨架屏
  if (loading && !usage) {
    return (
      <div className={`flex items-center gap-2 px-3 py-2 rounded-lg border border-neutral-200 dark:border-neutral-700 bg-neutral-50 dark:bg-neutral-800/50 animate-pulse ${className}`}>
        <div className="h-3 w-3 rounded-full bg-neutral-300 dark:bg-neutral-600" />
        <div className="h-3 w-24 rounded bg-neutral-300 dark:bg-neutral-600" />
      </div>
    )
  }

  // 错误状态
  if (error) {
    return (
      <div className={`flex items-center gap-2 px-3 py-2 rounded-lg border border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-900/20 ${className}`}>
        <AlertCircle size={14} className="text-red-600 dark:text-red-400" />
        <span className="text-xs text-red-600 dark:text-red-400">
          {lang === 'zh' ? '无法加载用量' : 'Failed to load usage'}
        </span>
      </div>
    )
  }

  // 没有用量数据
  if (!usage) {
    return null
  }

  const consumedQuota = usage.consumed_quota || 0
  // The task endpoint is authoritative. A zero value explicitly means that
  // the server disabled that guard, so do not fall back to stale UI defaults.
  const effectiveSoftCap = usage.soft_cap ?? softCap ?? 0
  const effectiveHardCap = usage.hard_cap ?? hardCap ?? 0
  const totalTokens = usage.total_tokens || 0
  const usageLevel = getUsageLevel(consumedQuota, effectiveSoftCap, effectiveHardCap)
  const percentage = effectiveHardCap > 0
    ? Math.min((consumedQuota / effectiveHardCap) * 100, 100)
    : 0

  // 硬上限已达到
  const isBlocked = effectiveHardCap > 0 && (usage.hard_cap_exceeded || consumedQuota >= effectiveHardCap)
  const hasCap = effectiveHardCap > 0

  return (
    <div className={`flex flex-col gap-2 ${className}`}>
      {/* 主要指示器 */}
      <div className={`flex items-center justify-between gap-3 px-3 py-2 rounded-lg border ${usageLevel.bgColor}`}>
        <div className="flex items-center gap-2 flex-1 min-w-0">
          <div className={usageLevel.color}>
            {usageLevel.icon}
          </div>
          <div className="flex items-center gap-2 text-xs flex-1 min-w-0">
            <span className={`font-medium ${usageLevel.color}`}>
              {formatQuota(consumedQuota, lang)}
            </span>
            {hasCap && (
              <>
                <span className="text-neutral-500 dark:text-neutral-400">/</span>
                <span className="text-neutral-600 dark:text-neutral-400">
                  {formatQuota(effectiveHardCap, lang)}
                </span>
              </>
            )}
            <span className="text-neutral-500 dark:text-neutral-400 truncate">
              ({lang === 'zh' ? `处理 ${totalTokens.toLocaleString()} tokens` : `${totalTokens.toLocaleString()} tokens processed`})
            </span>
          </div>
        </div>
      </div>

      {/* 进度条 */}
      {hasCap && (
        <div className="h-1 w-full rounded-full bg-neutral-200 dark:bg-neutral-700 overflow-hidden">
          <div
            className={`h-full transition-all duration-300 ${
              isBlocked
                ? 'bg-red-500 dark:bg-red-400'
                : usageLevel.level === 'warning'
                  ? 'bg-yellow-500 dark:bg-yellow-400'
                  : 'bg-blue-500 dark:bg-blue-400'
            }`}
            style={{ width: `${percentage}%` }}
          />
        </div>
      )}

      {/* 警告信息 */}
      {usageLevel.level === 'warning' && !isBlocked && (
        <div className="flex items-start gap-2 px-3 py-2 rounded-lg bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800">
          <AlertCircle size={14} className="text-yellow-600 dark:text-yellow-400 mt-0.5 flex-shrink-0" />
          <div className="text-xs text-yellow-700 dark:text-yellow-300">
            {lang === 'zh'
              ? `累计额度已达到软上限（${formatQuota(effectiveSoftCap, lang)}），任务仍会继续执行。`
              : `Soft cap reached (${formatQuota(effectiveSoftCap, lang)}). Please monitor usage.`
            }
          </div>
        </div>
      )}

      {/* 阻断信息 */}
      {isBlocked && (
        <div className="flex items-start gap-2 px-3 py-2 rounded-lg bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800">
          <AlertCircle size={14} className="text-red-600 dark:text-red-400 mt-0.5 flex-shrink-0" />
          <div className="text-xs text-red-700 dark:text-red-300">
            {lang === 'zh'
              ? `累计额度已达到硬上限（${formatQuota(effectiveHardCap, lang)}），任务将在下一轮请求前停止。`
              : `Hard cap reached (${formatQuota(effectiveHardCap, lang)}). Cannot send more messages.`
            }
          </div>
        </div>
      )}
    </div>
  )
}
