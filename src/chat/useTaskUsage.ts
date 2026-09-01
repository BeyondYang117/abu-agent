import { useEffect, useState, useCallback, useRef } from 'react'
import * as abuApi from '../api/abuApi'

export interface TaskUsage {
  task_id: string
  consumed_quota: number
  total_tokens: number
  prompt_tokens: number
  completion_tokens: number
  soft_cap_exceeded: boolean
  hard_cap_exceeded: boolean
  status: string
}

interface UseTaskUsageOptions {
  /** 轮询间隔（毫秒），默认 5000ms */
  pollInterval?: number
  /** 是否启用轮询，默认 true */
  enabled?: boolean
  /** 错误重试次数，默认 3 */
  retryCount?: number
  /** 重试延迟（毫秒），默认 1000ms */
  retryDelay?: number
}

interface UseTaskUsageReturn {
  usage: TaskUsage | null
  loading: boolean
  error: string | null
  refresh: () => Promise<void>
  isPolling: boolean
}

const CACHE_TTL = 5000 // 缓存 5 秒
const usageCache = new Map<string, { data: TaskUsage; timestamp: number }>()

/**
 * 实时任务用量 Hook
 *
 * 功能：
 * - 轮询获取任务用量数据
 * - 自动缓存优化
 * - 错误重试机制
 * - 自动检测硬上限和软上限
 * - 组件卸载时自动清理
 *
 * @example
 * ```tsx
 * const { usage, loading, error } = useTaskUsage(taskId)
 *
 * if (usage?.hard_cap_exceeded) {
 *   // 禁用输入
 * }
 * ```
 */
export function useTaskUsage(
  taskId: string | null,
  options: UseTaskUsageOptions = {}
): UseTaskUsageReturn {
  const {
    pollInterval = 5000,
    enabled = true,
    retryCount = 3,
    retryDelay = 1000
  } = options

  const [usage, setUsage] = useState<TaskUsage | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [isPolling, setIsPolling] = useState(false)

  const pollingTimerRef = useRef<NodeJS.Timeout | null>(null)
  const retryTimerRef = useRef<NodeJS.Timeout | null>(null)
  const retryAttemptsRef = useRef(0)
  const isMountedRef = useRef(true)

  // 从缓存获取数据
  const getFromCache = useCallback((id: string): TaskUsage | null => {
    const cached = usageCache.get(id)
    if (!cached) return null

    const now = Date.now()
    if (now - cached.timestamp > CACHE_TTL) {
      usageCache.delete(id)
      return null
    }

    return cached.data
  }, [])

  // 保存到缓存
  const saveToCache = useCallback((id: string, data: TaskUsage) => {
    usageCache.set(id, {
      data,
      timestamp: Date.now()
    })
  }, [])

  // 获取任务用量
  const fetchUsage = useCallback(async (id: string, isRetry = false): Promise<void> => {
    if (!isMountedRef.current) return

    try {
      // 首次加载时显示 loading
      if (!isRetry && !usage) {
        setLoading(true)
      }
      setError(null)

      // 尝试从缓存获取
      const cached = getFromCache(id)
      if (cached && !isRetry) {
        setUsage(cached)
        setLoading(false)
        return
      }

      // 调用 API
      const data = await abuApi.getTaskUsage(id)

      if (!isMountedRef.current) return

      // 计算是否超过上限（假设默认上限）
      const softCap = 500
      const hardCap = 2000
      const enrichedData: TaskUsage = {
        ...data,
        soft_cap_exceeded: data.consumed_quota >= softCap,
        hard_cap_exceeded: data.consumed_quota >= hardCap
      }

      setUsage(enrichedData)
      saveToCache(id, enrichedData)
      setLoading(false)
      retryAttemptsRef.current = 0 // 成功后重置重试次数

    } catch (err) {
      if (!isMountedRef.current) return

      console.error('Failed to fetch task usage:', err)

      // 重试逻辑
      if (retryAttemptsRef.current < retryCount) {
        retryAttemptsRef.current++
        retryTimerRef.current = setTimeout(() => {
          fetchUsage(id, true)
        }, retryDelay * retryAttemptsRef.current) // 递增延迟
      } else {
        setError(err instanceof Error ? err.message : 'Failed to fetch usage')
        setLoading(false)
      }
    }
  }, [usage, getFromCache, saveToCache, retryCount, retryDelay])

  // 手动刷新
  const refresh = useCallback(async () => {
    if (!taskId) return
    retryAttemptsRef.current = 0
    await fetchUsage(taskId, false)
  }, [taskId, fetchUsage])

  // 启动轮询
  const startPolling = useCallback(() => {
    if (!taskId || !enabled) return

    setIsPolling(true)

    // 立即获取一次
    fetchUsage(taskId)

    // 设置轮询定时器
    pollingTimerRef.current = setInterval(() => {
      fetchUsage(taskId)
    }, pollInterval)
  }, [taskId, enabled, pollInterval, fetchUsage])

  // 停止轮询
  const stopPolling = useCallback(() => {
    setIsPolling(false)

    if (pollingTimerRef.current) {
      clearInterval(pollingTimerRef.current)
      pollingTimerRef.current = null
    }

    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current)
      retryTimerRef.current = null
    }
  }, [])

  // 启动/停止轮询
  useEffect(() => {
    if (taskId && enabled) {
      startPolling()
    } else {
      stopPolling()
    }

    return () => {
      stopPolling()
    }
  }, [taskId, enabled, startPolling, stopPolling])

  // 组件卸载清理
  useEffect(() => {
    return () => {
      isMountedRef.current = false
      stopPolling()
    }
  }, [stopPolling])

  return {
    usage,
    loading,
    error,
    refresh,
    isPolling
  }
}

/**
 * 清除所有缓存
 */
export function clearTaskUsageCache() {
  usageCache.clear()
}

/**
 * 清除特定任务的缓存
 */
export function clearTaskUsageCacheById(taskId: string) {
  usageCache.delete(taskId)
}
