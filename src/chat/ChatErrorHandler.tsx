import { AlertTriangle, WifiOff, Clock, RefreshCw, CreditCard } from 'lucide-react'
import { Button } from '../components/Button'

export type ChatErrorType =
  | 'network'
  | 'token_expired'
  | 'quota_exhausted'
  | 'task_cancelled'
  | 'unknown'

interface ChatErrorHandlerProps {
  error: {
    type: ChatErrorType
    message?: string
  }
  lang: 'zh' | 'en'
  onRetry?: () => void
  onSwitchToLocal?: () => void
  onRecharge?: () => void
  onLogin?: () => void
  className?: string
}

interface ErrorConfig {
  icon: React.ReactNode
  title: string
  description: string
  actions: Array<{
    label: string
    variant: 'primary' | 'secondary'
    onClick?: () => void
  }>
}

function getErrorConfig(
  type: ChatErrorType,
  lang: 'zh' | 'en',
  handlers: {
    onRetry?: () => void
    onSwitchToLocal?: () => void
    onRecharge?: () => void
    onLogin?: () => void
  }
): ErrorConfig {
  switch (type) {
    case 'network':
      return {
        icon: <WifiOff size={24} className="text-red-500" />,
        title: lang === 'zh' ? '网络连接失败' : 'Network Connection Failed',
        description: lang === 'zh'
          ? '无法连接到服务器，请检查网络连接后重试。'
          : 'Cannot connect to server. Please check your network connection and retry.',
        actions: [
          {
            label: lang === 'zh' ? '重试' : 'Retry',
            variant: 'primary',
            onClick: handlers.onRetry
          },
          ...(handlers.onSwitchToLocal ? [{
            label: lang === 'zh' ? '切换到本地模式' : 'Switch to Local Mode',
            variant: 'secondary' as const,
            onClick: handlers.onSwitchToLocal
          }] : [])
        ]
      }

    case 'token_expired':
      return {
        icon: <Clock size={24} className="text-yellow-500" />,
        title: lang === 'zh' ? '登录已过期' : 'Session Expired',
        description: lang === 'zh'
          ? '您的登录状态已过期，请重新登录后继续使用。'
          : 'Your session has expired. Please log in again to continue.',
        actions: [
          {
            label: lang === 'zh' ? '重新登录' : 'Log In Again',
            variant: 'primary',
            onClick: handlers.onLogin
          }
        ]
      }

    case 'quota_exhausted':
      return {
        icon: <CreditCard size={24} className="text-orange-500" />,
        title: lang === 'zh' ? '配额已用尽' : 'Quota Exhausted',
        description: lang === 'zh'
          ? '您的可用配额已用尽，无法继续发送消息。请充值后继续使用。'
          : 'Your available quota has been exhausted. Please recharge to continue.',
        actions: [
          {
            label: lang === 'zh' ? '立即充值' : 'Recharge Now',
            variant: 'primary',
            onClick: handlers.onRecharge
          },
          ...(handlers.onSwitchToLocal ? [{
            label: lang === 'zh' ? '切换到本地模式' : 'Switch to Local Mode',
            variant: 'secondary' as const,
            onClick: handlers.onSwitchToLocal
          }] : [])
        ]
      }

    case 'task_cancelled':
      return {
        icon: <AlertTriangle size={24} className="text-gray-500" />,
        title: lang === 'zh' ? '任务已取消' : 'Task Cancelled',
        description: lang === 'zh'
          ? '当前任务已被取消，状态已清理。'
          : 'The current task has been cancelled and the state has been cleaned up.',
        actions: []
      }

    case 'unknown':
    default:
      return {
        icon: <AlertTriangle size={24} className="text-red-500" />,
        title: lang === 'zh' ? '发生错误' : 'An Error Occurred',
        description: lang === 'zh'
          ? '处理您的请求时发生错误，请稍后重试。'
          : 'An error occurred while processing your request. Please try again later.',
        actions: [
          {
            label: lang === 'zh' ? '重试' : 'Retry',
            variant: 'primary',
            onClick: handlers.onRetry
          }
        ]
      }
  }
}

export function ChatErrorHandler({
  error,
  lang,
  onRetry,
  onSwitchToLocal,
  onRecharge,
  onLogin,
  className = ''
}: ChatErrorHandlerProps) {
  const config = getErrorConfig(error.type, lang, {
    onRetry,
    onSwitchToLocal,
    onRecharge,
    onLogin
  })

  return (
    <div className={`flex flex-col items-center justify-center p-6 ${className}`}>
      <div className="flex flex-col items-center gap-4 max-w-md text-center">
        {/* 图标 */}
        <div className="flex items-center justify-center w-16 h-16 rounded-full bg-neutral-100 dark:bg-neutral-800">
          {config.icon}
        </div>

        {/* 标题 */}
        <div>
          <h3 className="text-lg font-semibold text-neutral-900 dark:text-neutral-100 mb-2">
            {config.title}
          </h3>
          <p className="text-sm text-neutral-600 dark:text-neutral-400">
            {config.description}
          </p>
          {error.message && (
            <p className="text-xs text-neutral-500 dark:text-neutral-500 mt-2 font-mono">
              {error.message}
            </p>
          )}
        </div>

        {/* 操作按钮 */}
        {config.actions.length > 0 && (
          <div className="flex flex-wrap items-center justify-center gap-2 mt-2">
            {config.actions.map((action, index) => (
              <Button
                key={index}
                size="sm"
                variant={action.variant === 'primary' ? 'default' : 'ghost'}
                onClick={action.onClick}
                data-tauri-drag-region="false"
              >
                {action.variant === 'primary' && action.onClick === onRetry && (
                  <RefreshCw size={14} />
                )}
                {action.label}
              </Button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
