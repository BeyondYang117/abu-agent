import { memo, useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { ChevronDown } from 'lucide-react'
import type { AgentPlanState, ChatMessage } from './types'
import { MessageBubble } from './MessageBubble'
import { useStreamCoarse, useStreamSnapshot } from './streamingStore'
import { prefersReducedMotion } from './utils'

export interface AssistantStreamStats {
  messageId: string
  tokensPerSec: number
  reasoningDurationMs?: number | null
  reasoningDurationMsBySegmentId?: Record<string, number>
}

interface MessageListProps {
  conversationId?: string | null
  messages: ChatMessage[]
  agentPlanState?: AgentPlanState | null
  assistantStreamStatsByMessageId?: Record<string, AssistantStreamStats>
  onUpdateMessage?: (messageId: string, content: string) => Promise<void>
  onRegenerateMessage?: (messageId: string) => Promise<void>
  onDeleteMessage?: (messageId: string) => Promise<void>
}

// 列表里每一项的统一形态。整条会话全量喂给虚拟列表（消息都在内存，virtua 只渲可见项），
// 屏外的气泡连同其 KaTeX DOM 真正从 DOM 卸载——这是消除公式滚动卡顿的根治手段。
// 流式预览 bubble / 「正在思考」/ error 占位也作为列表尾项参与虚拟化与变高测量，
// 这样钉底逻辑只需把视口对齐到最后一项即可。
type RenderItem =
  | { kind: 'plan'; key: 'agent-plan'; planState: AgentPlanState }
  | { kind: 'message'; key: string; message: ChatMessage }
  | { kind: 'streaming'; key: 'streaming-assistant'; message: ChatMessage; messageStreaming: boolean; reasoningStreaming: boolean }
  | { kind: 'thinking'; key: 'thinking' }
  | { kind: 'error'; key: 'error'; text: string }

function MessageListBase({
  conversationId,
  messages,
  agentPlanState = null,
  assistantStreamStatsByMessageId = {},
  onUpdateMessage,
  onRegenerateMessage,
  onDeleteMessage,
}: MessageListProps) {
  // 流式预览状态直接订阅 streamingStore——只有本组件随每帧内容重渲，Chat/侧栏/输入栏不动。
  const coarse = useStreamCoarse()
  const snapshot = useStreamSnapshot()
  const streaming = coarse.streaming
  const streamFrozen = coarse.streamFrozen
  const error = coarse.streamError
  const streamingContent = snapshot.content
  const streamingReasoning = snapshot.reasoning
  const streamingReasoningDurationMs = snapshot.reasoningDurationMs
  const streamingReasoningDurationMsBySegmentId = snapshot.reasoningDurationMsBySegmentId
  const reasoningStreaming = snapshot.reasoningStreaming
  const streamingToolCalls = snapshot.toolCalls
  const streamingSegments = snapshot.segments

  const scrollRef = useRef<HTMLDivElement>(null)
  // 用户是否“贴在底部”——决定流式生成时是否跟随钉底。默认 true（初次渲染贴底）
  const stickToBottomRef = useRef(true)
  const prevMessageCountRef = useRef(0)
  // 是否贴在底部——驱动「回到底部」按钮的显隐（ref 不触发渲染，故另用 state）
  const [atBottom, setAtBottom] = useState(true)
  const lastScrollOffsetRef = useRef(0)

  // 把 Agent plan + 消息 + 流式预览 + 占位拼成统一的虚拟列表项数组。
  const items = useMemo<RenderItem[]>(() => {
    const list: RenderItem[] = []

    if (agentPlanState?.plan?.trim()) {
      list.push({ kind: 'plan', key: 'agent-plan', planState: agentPlanState })
    }

    for (const message of messages) {
      list.push({ kind: 'message', key: message.id, message })
    }
    const hasStreamingPreview =
      (streaming || streamFrozen) &&
      (streamingContent || streamingReasoning || streamingToolCalls.length > 0 || streamingSegments.length > 0)
    if (hasStreamingPreview) {
      list.push({
        kind: 'streaming',
        key: 'streaming-assistant',
        messageStreaming: streaming && !streamFrozen,
        reasoningStreaming: reasoningStreaming && !streamFrozen,
        message: {
          id: 'streaming-assistant',
          role: 'assistant',
          content: streamingContent,
          reasoning: streamingReasoning || undefined,
          artifacts: [],
          tool_calls: streamingToolCalls,
          segments: streamingSegments,
          timestamp: Math.floor(Date.now() / 1000),
        },
      })
    } else if (streaming) {
      list.push({ kind: 'thinking', key: 'thinking' })
    }

    if (error) {
      list.push({ kind: 'error', key: 'error', text: error })
    }
    return list
  }, [
    agentPlanState,
    messages,
    streaming,
    streamFrozen,
    streamingContent,
    streamingReasoning,
    reasoningStreaming,
    streamingToolCalls,
    streamingSegments,
    error,
  ])

  // 瞬时把视口对齐到底部。自动跟随保持瞬时（平滑会抖）；smooth 仅用于用户主动点「回到底部」。
  const scrollToBottom = useCallback((smooth = false) => {
    const el = scrollRef.current
    if (!el) return
    if (smooth && !prefersReducedMotion()) { el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' }); return }
    el.scrollTop = el.scrollHeight
    lastScrollOffsetRef.current = el.scrollTop
  }, [])

  const handleJumpToBottom = useCallback(() => {
    stickToBottomRef.current = true
    setAtBottom(true)
    scrollToBottom(true)
  }, [scrollToBottom])

  // 滚轮向上 = 明确的离开底部意图，立即解除跟随（不设缓冲，消除“挣扎感”）
  const handleWheel = (e: React.WheelEvent) => {
    if (e.deltaY < 0) {
      stickToBottomRef.current = false
      setAtBottom(false)
    }
  }

  // 滚动监听：用原生 scrollTop 判断贴底/离开底部。
  const handleScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    const offset = el.scrollTop
    const bottom = el.scrollHeight - offset - el.clientHeight <= 32
    if (offset < lastScrollOffsetRef.current - 1) {
      stickToBottomRef.current = false
    } else if (bottom) {
      stickToBottomRef.current = true
    }
    lastScrollOffsetRef.current = offset
    setAtBottom(bottom)
  }, [])

  // 切换会话：重置跟随并瞬间定位到底部
  useLayoutEffect(() => {
    stickToBottomRef.current = true
    setAtBottom(true)
    // 等虚拟列表用最新 items 渲染后再对齐底部
    requestAnimationFrame(() => scrollToBottom())
    // 仅在 conversationId 变化时重置；scrollToBottom 依赖 items.length，故不列入依赖避免误触发
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [conversationId])

  // 自己发出新消息时强制回到底部（即使刚才正往上翻历史）
  useLayoutEffect(() => {
    const count = messages.length
    if (count > prevMessageCountRef.current && messages[count - 1]?.role === 'user') {
      stickToBottomRef.current = true
      setAtBottom(true)
    }
    prevMessageCountRef.current = count
  }, [messages])

  // 仅在“贴底”时随内容增长钉住底部。virtua 内置 ResizeObserver 会在变高（KaTeX/图片
  // mount 后撑高）时重测，这里在每次内容/项数变化后重新对齐末尾，保证持续钉底。
  useLayoutEffect(() => {
    if (!stickToBottomRef.current) return
    scrollToBottom()
  }, [
    items,
    streaming,
    streamingContent,
    streamingReasoning,
    reasoningStreaming,
    streamingToolCalls,
    streamingSegments,
    scrollToBottom,
  ])

  const renderItem = useCallback(
    (item: RenderItem) => {
      switch (item.kind) {
        case 'plan':
          return <AgentPlanPanel planState={item.planState} />
        case 'message': {
          const msg = item.message
          const assistantStats = msg.role === 'assistant'
            ? assistantStreamStatsByMessageId[msg.id]
            : undefined
          return (
            <MessageBubble
              message={msg}
              conversationId={conversationId}
              tokensPerSec={assistantStats?.tokensPerSec}
              reasoningDurationMs={assistantStats?.reasoningDurationMs}
              reasoningDurationMsBySegmentId={assistantStats?.reasoningDurationMsBySegmentId}
              onUpdateMessage={msg.role === 'assistant' ? onUpdateMessage : undefined}
              onRegenerateMessage={msg.role === 'assistant' ? onRegenerateMessage : undefined}
              onDeleteMessage={onDeleteMessage}
            />
          )
        }
        case 'streaming':
          return (
            <MessageBubble
              message={item.message}
              conversationId={conversationId}
              messageStreaming={item.messageStreaming}
              reasoningStreaming={item.reasoningStreaming}
              reasoningDurationMs={streamingReasoningDurationMs}
              reasoningDurationMsBySegmentId={streamingReasoningDurationMsBySegmentId}
            />
          )
        case 'thinking':
          return (
            <div className="chat-motion-fade-up flex justify-start py-3">
              <span className="reasoning-shimmer-text text-sm font-medium">正在思考…</span>
            </div>
          )
        case 'error':
          return (
            <div className="chat-motion-fade-up flex justify-start py-3">
              <p className="max-w-[85%] text-sm leading-relaxed text-red-600 dark:text-red-400">
                {item.text}
              </p>
            </div>
          )
      }
    },
    [
      conversationId,
      assistantStreamStatsByMessageId,
      onUpdateMessage,
      onRegenerateMessage,
      onDeleteMessage,
      streamingReasoningDurationMs,
      streamingReasoningDurationMsBySegmentId,
    ],
  )

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        onWheel={handleWheel}
        className="chat-motion-fade custom-scrollbar flex-1 overflow-y-auto"
      >
        <div className="chat-message-list-inner mx-auto w-full max-w-3xl space-y-0.5 px-6 py-4">
          {/* 全量渲染：不虚拟化。滚动时绝不卸载/重挂消息——避免重挂 KaTeX 子树触发的
              matchAllRules 全量样式重算风暴（实测公式滚动卡顿的真正根因）。 */}
          {items.map((item) => (
            <div key={item.key}>{renderItem(item)}</div>
          ))}
        </div>
      </div>
      {!atBottom && (
        <button
          type="button"
          onClick={handleJumpToBottom}
          aria-label="回到底部"
          title="回到底部"
          className="chat-motion-pop absolute bottom-4 left-1/2 z-10 flex h-9 w-9 -translate-x-1/2 items-center justify-center rounded-full border border-neutral-200 bg-white/95 text-neutral-600 shadow-md backdrop-blur transition-transform duration-[var(--kv-dur-instant)] ease-[var(--kv-ease-spring)] hover:text-neutral-900 active:scale-90 dark:border-neutral-700 dark:bg-neutral-900/95 dark:text-neutral-300 dark:hover:text-neutral-100"
        >
          <ChevronDown size={18} strokeWidth={2} />
        </button>
      )}
    </div>
  )
}

function AgentPlanPanel({ planState }: { planState?: AgentPlanState | null }) {
  const plan = planState?.plan?.trim() ?? ''
  if (!plan) return null

  const mode = planState?.mode ?? 'act'
  const status = planState?.status ?? 'draft'
  return (
    <div className="chat-motion-fade-up pb-3 pt-1">
      <section className="border-b border-[var(--theme-surface-border)] bg-[color-mix(in_srgb,var(--theme-surface)_95%,transparent)] pb-3 backdrop-blur dark:border-neutral-800/80 dark:bg-[#212121]/95">
        <div className="mb-2 flex items-center justify-between gap-3">
          <div className="text-[11px] font-medium uppercase tracking-normal text-neutral-500 dark:text-neutral-400">
            Agent plan
          </div>
          <div className="text-[11px] text-neutral-400 dark:text-neutral-500">
            {status} · {mode}
          </div>
        </div>
        <p className="line-clamp-6 whitespace-pre-wrap text-[12px] leading-relaxed text-neutral-700 dark:text-neutral-300">
          {plan}
        </p>
      </section>
    </div>
  )
}

// memo：列表本身订阅 streamingStore，父级 Chat 重渲（非流式 state 变化）时不跟着白渲。
export const MessageList = memo(MessageListBase)
