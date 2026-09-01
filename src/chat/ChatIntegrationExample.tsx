import { useState, useEffect } from 'react'
import { useTaskUsage } from './useTaskUsage'
import { TaskQuotaIndicator } from './TaskQuotaIndicator'
import { ChatErrorHandler, inferErrorType, type ChatErrorType } from './ChatErrorHandler'
import type { Settings } from '../api/tauri'
import * as api from '../api/tauri'

/**
 * Chat 组件集成示例
 *
 * 展示如何将 Phase 5 的功能集成到现有的 Chat 组件中
 */

interface ChatIntegrationExampleProps {
  conversationId: string
  settings: Settings
  lang: 'zh' | 'en'
  onError?: (error: any) => void
}

export function ChatIntegrationExample({
  conversationId,
  settings,
  lang,
  onError
}: ChatIntegrationExampleProps) {
  const [currentTaskId, setCurrentTaskId] = useState<string | null>(null)
  const [error, setError] = useState<{ type: ChatErrorType; message?: string } | null>(null)
  const isCloudMode = settings.runtime_mode === 'cloud'

  // 使用 Task Usage Hook
  const { usage, loading, error: usageError } = useTaskUsage(
    isCloudMode ? currentTaskId : null,
    {
      pollInterval: 5000,
      enabled: isCloudMode && !!currentTaskId,
      retryCount: 3
    }
  )

  // 监听用量错误
  useEffect(() => {
    if (usageError) {
      const errorType = inferErrorType(usageError)
      setError({ type: errorType, message: usageError })
      onError?.(usageError)
    }
  }, [usageError, onError])

  // 检查是否达到硬上限
  const isBlocked = usage?.hard_cap_exceeded ?? false

  // 错误处理函数
  const handleRetry = async () => {
    setError(null)
    // 重新加载或重试逻辑
  }

  const handleSwitchToLocal = async () => {
    if (confirm(lang === 'zh' ? '切换到本地模式？' : 'Switch to local mode?')) {
      await api.updateSettings({ runtime_mode: 'local' })
      window.location.reload()
    }
  }

  const handleRecharge = () => {
    // 打开充值页面
    const baseUrl = settings.abu_api_base_url || 'https://api.example.com'
    window.open(`${baseUrl}/recharge`, '_blank')
  }

  const handleLogin = () => {
    // 跳转到登录页面
    window.location.href = '/onboarding?step=login'
  }

  return (
    <div className="flex flex-col h-full">
      {/* 聊天消息区域 */}
      <div className="flex-1 overflow-y-auto">
        {/* 现有的聊天消息渲染 */}
      </div>

      {/* 错误显示 */}
      {error && (
        <div className="border-t border-neutral-200 dark:border-neutral-700">
          <ChatErrorHandler
            error={error}
            lang={lang}
            onRetry={handleRetry}
            onSwitchToLocal={isCloudMode ? handleSwitchToLocal : undefined}
            onRecharge={handleRecharge}
            onLogin={handleLogin}
          />
        </div>
      )}

      {/* 输入区域 */}
      <div className="border-t border-neutral-200 dark:border-neutral-700">
        {/* Cloud 模式下显示配额指示器 */}
        {isCloudMode && currentTaskId && (
          <div className="px-4 pt-3">
            <TaskQuotaIndicator
              taskId={currentTaskId}
              softCap={500}
              hardCap={2000}
              lang={lang}
            />
          </div>
        )}

        {/* 输入框 */}
        <div className="p-4">
          <textarea
            className="w-full rounded-lg border border-neutral-300 dark:border-neutral-600 p-3"
            placeholder={
              isBlocked
                ? lang === 'zh'
                  ? '已达到配额上限，无法发送'
                  : 'Quota limit reached, cannot send'
                : lang === 'zh'
                  ? '输入消息...'
                  : 'Type a message...'
            }
            disabled={isBlocked || loading}
            rows={3}
          />
          <div className="flex justify-end mt-2">
            <button
              className="px-4 py-2 rounded-lg bg-blue-600 text-white disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={isBlocked || loading}
            >
              {lang === 'zh' ? '发送' : 'Send'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

/**
 * 集成步骤说明：
 *
 * 1. 在 Chat.tsx 中导入必要的组件和 Hook：
 *    ```typescript
 *    import { useTaskUsage } from './useTaskUsage'
 *    import { TaskQuotaIndicator } from './TaskQuotaIndicator'
 *    import { ChatErrorHandler, inferErrorType } from './ChatErrorHandler'
 *    ```
 *
 * 2. 在 Chat 组件中添加状态管理：
 *    ```typescript
 *    const [currentTaskId, setCurrentTaskId] = useState<string | null>(null)
 *    const [error, setError] = useState<{ type: ChatErrorType; message?: string } | null>(null)
 *    const isCloudMode = settings.runtime_mode === 'cloud'
 *    ```
 *
 * 3. 使用 useTaskUsage Hook：
 *    ```typescript
 *    const { usage, loading, error: usageError } = useTaskUsage(
 *      isCloudMode ? currentTaskId : null,
 *      { pollInterval: 5000, enabled: isCloudMode && !!currentTaskId }
 *    )
 *    ```
 *
 * 4. 在 InputBar 上方显示 TaskQuotaIndicator：
 *    ```typescript
 *    {isCloudMode && currentTaskId && (
 *      <TaskQuotaIndicator
 *        taskId={currentTaskId}
 *        softCap={500}
 *        hardCap={2000}
 *        lang={lang}
 *      />
 *    )}
 *    ```
 *
 * 5. 根据配额状态禁用输入：
 *    ```typescript
 *    const isBlocked = usage?.hard_cap_exceeded ?? false
 *    <InputBar disabled={isBlocked || loading} />
 *    ```
 *
 * 6. 处理错误并显示 ChatErrorHandler：
 *    ```typescript
 *    {error && (
 *      <ChatErrorHandler
 *        error={error}
 *        lang={lang}
 *        onRetry={handleRetry}
 *        onSwitchToLocal={handleSwitchToLocal}
 *        onRecharge={handleRecharge}
 *        onLogin={handleLogin}
 *      />
 *    )}
 *    ```
 *
 * 7. 在发送消息时更新 currentTaskId：
 *    ```typescript
 *    const handleSendMessage = async (message: string) => {
 *      try {
 *        const response = await chatApi.sendMessage(conversationId, message)
 *        if (response.task_id) {
 *          setCurrentTaskId(response.task_id)
 *        }
 *      } catch (err) {
 *        const errorType = inferErrorType(err)
 *        setError({ type: errorType, message: err.message })
 *      }
 *    }
 *    ```
 */
