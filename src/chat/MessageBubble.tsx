import type { ChatMessage } from './types'

interface MessageBubbleProps {
  message: ChatMessage
}

export function MessageBubble({ message }: MessageBubbleProps) {
  const isUser = message.role === 'user'

  return (
    <div className={`flex ${isUser ? 'justify-end' : 'justify-start'} mb-4`}>
      <div
        className={`max-w-[70%] rounded-2xl px-4 py-3 ${
          isUser
            ? 'bg-blue-500 text-white'
            : 'bg-neutral-100 dark:bg-neutral-800 text-neutral-900 dark:text-neutral-100'
        }`}
      >
        {/* 推理内容（如果有） */}
        {message.reasoning && !isUser && (
          <div className="mb-2 pb-2 border-b border-neutral-300 dark:border-neutral-700">
            <div className="text-xs text-neutral-500 dark:text-neutral-400 mb-1">思考过程</div>
            <div className="text-sm opacity-80 whitespace-pre-wrap">{message.reasoning}</div>
          </div>
        )}

        {/* 消息内容 */}
        <div className="text-[15px] leading-relaxed whitespace-pre-wrap break-words">
          {message.content}
        </div>

        {/* 附件显示 */}
        {message.attachments && message.attachments.length > 0 && (
          <div className="mt-2 space-y-2">
            {message.attachments.map((att) => (
              <div
                key={att.id}
                className="flex items-center gap-2 p-2 rounded bg-black/10 dark:bg-white/10 text-sm"
              >
                {att.type === 'image' ? '🖼️' : '📎'} {att.name}
              </div>
            ))}
          </div>
        )}

        {/* 时间戳 */}
        <div
          className={`text-xs mt-1 ${
            isUser ? 'text-blue-100' : 'text-neutral-400 dark:text-neutral-500'
          }`}
        >
          {new Date(message.timestamp * 1000).toLocaleTimeString('zh-CN', {
            hour: '2-digit',
            minute: '2-digit',
          })}
        </div>
      </div>
    </div>
  )
}
