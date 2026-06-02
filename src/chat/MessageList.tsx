import { useEffect, useRef } from 'react'
import type { ChatMessage } from './types'
import { MessageBubble } from './MessageBubble'

interface MessageListProps {
  messages: ChatMessage[]
  streaming?: boolean
  streamingContent?: string
  streamingReasoning?: string
  error?: string
}

export function MessageList({
  messages,
  streaming,
  streamingContent = '',
  streamingReasoning = '',
  error,
}: MessageListProps) {
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [messages, streaming, streamingContent, streamingReasoning, error])

  return (
    <div ref={scrollRef} className="custom-scrollbar flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-3xl space-y-1 px-6 py-4">
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}

        {streaming && (streamingContent || streamingReasoning) && (
          <MessageBubble
            message={{
              id: 'streaming-assistant',
              role: 'assistant',
              content: streamingContent,
              reasoning: streamingReasoning || undefined,
              timestamp: Math.floor(Date.now() / 1000),
            }}
          />
        )}

        {streaming && !streamingContent && !streamingReasoning && (
          <div className="flex justify-start py-2">
            <div className="rounded-2xl border border-neutral-200/80 bg-white px-4 py-3 dark:border-neutral-700 dark:bg-neutral-900">
              <div className="flex items-center gap-2">
                <span className="flex gap-1">
                  <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-neutral-400" />
                  <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-neutral-400 [animation-delay:0.2s]" />
                  <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-neutral-400 [animation-delay:0.4s]" />
                </span>
                <span className="text-sm text-neutral-500">正在思考…</span>
              </div>
            </div>
          </div>
        )}

        {error && (
          <div className="flex justify-start py-2">
            <div className="max-w-[85%] rounded-2xl border border-red-200/80 bg-red-50 px-4 py-3 text-sm leading-relaxed text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300">
              {error}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
