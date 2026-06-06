import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { ChatMessage, ToolCallRecord } from './types'
import { MessageBubble } from './MessageBubble'

const INITIAL_VISIBLE_MESSAGES = 60
const LOAD_MORE_MESSAGES = 60

export interface AssistantStreamStats {
  messageId: string
  tokensPerSec: number
}

interface MessageListProps {
  conversationId?: string | null
  messages: ChatMessage[]
  streaming?: boolean
  streamingContent?: string
  streamingReasoning?: string
  reasoningStreaming?: boolean
  streamingToolCalls?: ToolCallRecord[]
  error?: string
  lastAssistantStreamStats?: AssistantStreamStats | null
  onUpdateMessage?: (messageId: string, content: string) => Promise<void>
  onRegenerateMessage?: (messageId: string) => Promise<void>
  onDeleteMessage?: (messageId: string) => Promise<void>
}

export function MessageList({
  conversationId,
  messages,
  streaming,
  streamingContent = '',
  streamingReasoning = '',
  reasoningStreaming = false,
  streamingToolCalls = [],
  error,
  lastAssistantStreamStats = null,
  onUpdateMessage,
  onRegenerateMessage,
  onDeleteMessage,
}: MessageListProps) {
  const scrollRef = useRef<HTMLDivElement>(null)
  // 用户是否“贴在底部”——决定流式生成时是否跟随钉底。默认 true（初次渲染贴底）
  const stickToBottomRef = useRef(true)
  const prevCountRef = useRef(0)
  const lastScrollTopRef = useRef(0)
  const pendingPrependScrollHeightRef = useRef<number | null>(null)
  const [visibleLimit, setVisibleLimit] = useState(INITIAL_VISIBLE_MESSAGES)

  const lastAssistantId = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i].role === 'assistant') return messages[i].id
    }
    return null
  }, [messages])

  const hiddenMessageCount = Math.max(0, messages.length - visibleLimit)
  const visibleMessages = hiddenMessageCount > 0
    ? messages.slice(hiddenMessageCount)
    : messages

  const loadOlderMessages = useCallback(() => {
    const el = scrollRef.current
    if (el) {
      pendingPrependScrollHeightRef.current = el.scrollHeight
      stickToBottomRef.current = false
    }
    setVisibleLimit((limit) => Math.min(messages.length, limit + LOAD_MORE_MESSAGES))
  }, [messages.length])

  // 滚轮向上 = 明确的离开底部意图，立即解除跟随（不设缓冲，消除“挣扎感”）
  const handleWheel = (e: React.WheelEvent) => {
    if (e.deltaY < 0) stickToBottomRef.current = false
  }

  // 监听滚动：向上移动立即解除跟随；仅当主动滚回几乎贴底（≤32px）时恢复跟随
  const handleScroll = () => {
    const el = scrollRef.current
    if (!el) return
    const { scrollTop, scrollHeight, clientHeight } = el
    if (scrollTop <= 24 && hiddenMessageCount > 0) {
      loadOlderMessages()
    }
    if (scrollTop < lastScrollTopRef.current - 1) {
      stickToBottomRef.current = false
    } else if (scrollHeight - scrollTop - clientHeight <= 32) {
      stickToBottomRef.current = true
    }
    lastScrollTopRef.current = scrollTop
  }

  // 切换会话：重置跟随并瞬间定位到底部
  useLayoutEffect(() => {
    setVisibleLimit(INITIAL_VISIBLE_MESSAGES)
    pendingPrependScrollHeightRef.current = null
    stickToBottomRef.current = true
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [conversationId])

  useLayoutEffect(() => {
    const previousScrollHeight = pendingPrependScrollHeightRef.current
    if (previousScrollHeight == null) return
    pendingPrependScrollHeightRef.current = null
    const el = scrollRef.current
    if (!el) return
    const delta = el.scrollHeight - previousScrollHeight
    if (delta > 0) {
      el.scrollTop += delta
      lastScrollTopRef.current = el.scrollTop
    }
  }, [visibleLimit])

  // 自己发出新消息时强制回到底部（即使刚才正往上翻历史）
  useLayoutEffect(() => {
    const count = messages.length
    if (count > prevCountRef.current && messages[count - 1]?.role === 'user') {
      stickToBottomRef.current = true
    }
    prevCountRef.current = count
  }, [messages])

  // 仅在“贴底”时随内容增长钉住底部；useLayoutEffect 保证绘制前完成，消除抽动
  useLayoutEffect(() => {
    if (!stickToBottomRef.current) return
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [messages, streaming, streamingContent, streamingReasoning, reasoningStreaming, streamingToolCalls, error])

  return (
    <div ref={scrollRef} onScroll={handleScroll} onWheel={handleWheel} className="custom-scrollbar flex-1 overflow-y-auto">
      <div className="chat-message-list-inner mx-auto w-full max-w-3xl space-y-0.5 px-6 py-4">
        {hiddenMessageCount > 0 && (
          <div className="flex justify-center py-2">
            <button
              type="button"
              onClick={loadOlderMessages}
              className="rounded-full border border-neutral-200 bg-white px-3 py-1.5 text-[12px] font-medium text-neutral-500 shadow-sm transition-colors hover:border-neutral-300 hover:text-neutral-700 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-400 dark:hover:border-neutral-600 dark:hover:text-neutral-200"
            >
              加载更早 {Math.min(hiddenMessageCount, LOAD_MORE_MESSAGES)} 条
            </button>
          </div>
        )}

        {visibleMessages.map((msg) => (
          <MessageBubble
            key={msg.id}
            message={msg}
            conversationId={conversationId}
            tokensPerSec={
              msg.role === 'assistant' &&
              msg.id === lastAssistantId &&
              lastAssistantStreamStats?.messageId === msg.id
                ? lastAssistantStreamStats.tokensPerSec
                : undefined
            }
            onUpdateMessage={msg.role === 'assistant' ? onUpdateMessage : undefined}
            onRegenerateMessage={msg.role === 'assistant' ? onRegenerateMessage : undefined}
            onDeleteMessage={onDeleteMessage}
          />
        ))}

        {streaming && (streamingContent || streamingReasoning || streamingToolCalls.length > 0) && (
          <MessageBubble
            message={{
              id: 'streaming-assistant',
              role: 'assistant',
              content: streamingContent,
              reasoning: streamingReasoning || undefined,
              artifacts: [],
              tool_calls: streamingToolCalls,
              timestamp: Math.floor(Date.now() / 1000),
            }}
            conversationId={conversationId}
            reasoningStreaming={reasoningStreaming}
          />
        )}

        {streaming && !streamingContent && !streamingReasoning && streamingToolCalls.length === 0 && (
          <div className="chat-motion-fade-up flex justify-start py-3">
            <span className="reasoning-shimmer-text text-sm font-medium">正在思考…</span>
          </div>
        )}

        {error && (
          <div className="chat-motion-fade-up flex justify-start py-3">
            <p className="max-w-[85%] text-sm leading-relaxed text-red-600 dark:text-red-400">
              {error}
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
