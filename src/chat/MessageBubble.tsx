import type { ChatMessage } from './types'

interface MessageBubbleProps {
  message: ChatMessage
}

export function MessageBubble({ message }: MessageBubbleProps) {
  const isUser = message.role === 'user'

  return (
    <div className={`flex py-2 ${isUser ? 'justify-end' : 'justify-start'}`}>
      <div
        className={`max-w-[85%] rounded-2xl px-4 py-3 ${
          isUser
            ? 'bg-neutral-900 text-white dark:bg-neutral-100 dark:text-neutral-900'
            : 'border border-neutral-200/80 bg-white text-neutral-900 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100'
        }`}
      >
        {message.reasoning && !isUser && (
          <div className="mb-2 border-b border-neutral-200 pb-2 dark:border-neutral-700">
            <div className="mb-1 text-xs text-neutral-500">思考过程</div>
            <div className="whitespace-pre-wrap text-sm opacity-80">{message.reasoning}</div>
          </div>
        )}

        <div className="whitespace-pre-wrap break-words text-[15px] leading-relaxed">
          {message.content}
        </div>

        {message.attachments && message.attachments.length > 0 && (
          <div className="mt-2 space-y-2">
            {message.attachments.map((att) => (
              <div
                key={att.id}
                className="flex items-center gap-2 rounded-lg bg-black/5 p-2 text-sm dark:bg-white/10"
              >
                {att.type === 'image' ? '🖼️' : '📎'} {att.name}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
