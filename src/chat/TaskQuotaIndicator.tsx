import { useEffect, useState } from 'react'
import { AlertCircle, TrendingUp, DollarSign } from 'lucide-react'
import { useTaskUsage } from './useTaskUsage'

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
  const percentage = (consumedQuota / hardCap) * 100

  if (consumedQuota >= hardCap) {
    return {
      level: 'danger',
      color: 'text-red-600 dark:text-red-400',
      bgColor: 'bg-red-50 dark:bg-red-900/20 border-red-300 dark:border-red-700',
      icon: <AlertCircle size={14} />
    }
  } else if (consumedQuota >= softCap) {
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
  // 假设 quota 单位是分（100分 = 1元）
  const yuan = (quota / 100).toFixed(2)
  return lang === 'zh' ? `¥${yuan}` : `$${yuan}`
}

export function TaskQuotaIndicator({
  taskId,
  softCap = 500,
  hardCap = 2000,
  lang,
  className = ''
}: TaskQuotaIndicatorProps) {
  const { usage, loading, error, refresh } = useTaskUsage(taskId)

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
  const totalTokens = usage.total_tokens || 0
  const usageLevel = getUsageLevel(consumedQuota, softCap, hardCap)
  const percentage = Math.min((consumedQuota / hardCap) * 100, 100)

  // 硬上限已达到
  const isBlocked = consumedQuota >= hardCap

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
            <span className="text-neutral-500 dark:text-neutral-400">/</span>
            <span className="text-neutral-600 dark:text-neutral-400">
              {formatQuota(hardCap, lang)}
            </span>
            <span className="text-neutral-500 dark:text-neutral-400 truncate">
              ({totalTokens.toLocaleString()} tokens)
            </span>
          </div>
        </div>
      </div>

      {/* 进度条 */}
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

      {/* 警告信息 */}
      {usageLevel.level === 'warning' && !isBlocked && (
        <div className="flex items-start gap-2 px-3 py-2 rounded-lg bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800">
          <AlertCircle size={14} className="text-yellow-600 dark:text-yellow-400 mt-0.5 flex-shrink-0" />
          <div className="text-xs text-yellow-700 dark:text-yellow-300">
            {lang === 'zh'
              ? `已达到软上限（${formatQuota(softCap, lang)}），建议注意用量。`
              : `Soft cap reached (${formatQuota(softCap, lang)}). Please monitor usage.`
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
              ? `已达到硬上限（${formatQuota(hardCap, lang)}），无法继续发送消息。`
              : `Hard cap reached (${formatQuota(hardCap, lang)}). Cannot send more messages.`
            }
          </div>
        </div>
      )}
    </div>
  )
}
