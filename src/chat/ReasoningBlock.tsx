import { useEffect, useRef, useState } from 'react'
import { ChevronDown } from 'lucide-react'

type ReasoningBlockProps = {
  reasoning: string
  /** 思维链正在流式写入 */
  streaming?: boolean
}

export function ReasoningBlock({ reasoning, streaming = false }: ReasoningBlockProps) {
  const collapsible = reasoning.trim().length > 0
  const [open, setOpen] = useState(false)
  const [contentPulse, setContentPulse] = useState(false)
  const [bodyMaxHeight, setBodyMaxHeight] = useState<number | null>(null)
  const userExpandedRef = useRef(false)
  const bodyRef = useRef<HTMLDivElement>(null)
  const scrollRef = useRef<HTMLDivElement>(null)

  const showCollapsed = collapsible && !open
  /** 生成完毕的折叠态只留标题行，正文完全隐藏 */
  const hideBody = !streaming && showCollapsed

  useEffect(() => {
    if (!streaming) return
    setContentPulse(true)
    const timer = window.setTimeout(() => setContentPulse(false), 220)
    return () => window.clearTimeout(timer)
  }, [reasoning, streaming])

  useEffect(() => {
    if (!streaming && collapsible && !userExpandedRef.current) {
      setOpen(false)
    }
  }, [streaming, collapsible])

  useEffect(() => {
    const body = bodyRef.current
    if (!body || !collapsible) {
      setBodyMaxHeight(null)
      return
    }
    setBodyMaxHeight(hideBody ? 0 : body.scrollHeight)
  }, [collapsible, open, reasoning, hideBody, streaming])

  useEffect(() => {
    if (!streaming || hideBody) return
    const scrollBox = scrollRef.current
    if (!scrollBox) return
    scrollBox.scrollTop = scrollBox.scrollHeight
  }, [reasoning, streaming, hideBody, open])

  const titleClass =
    'mb-1 flex w-full items-center gap-1 text-left text-[12.5px] font-medium text-neutral-700 transition-colors dark:text-neutral-200'
  const bodyClass = [
    'chat-motion-reasoning-body',
    streaming ? 'opacity-95' : 'opacity-90',
    showCollapsed ? 'is-collapsed' : 'is-open',
  ].join(' ')
  const scrollClass = [
    'reasoning-scroll-box custom-scrollbar',
    streaming ? 'is-streaming' : 'is-expanded',
    contentPulse ? 'reasoning-stream-tail' : '',
  ].join(' ')

  const handleToggle = () => {
    userExpandedRef.current = true
    setOpen((value) => !value)
  }
  const visibleReasoning = reasoning.trimEnd()

  return (
    <section
      aria-label="Thinking"
      className={`mb-3 border-l pl-3 transition-colors duration-300 ${
        streaming
          ? 'border-neutral-300 dark:border-neutral-600'
          : 'border-neutral-200 dark:border-neutral-700'
      }`}
    >
      {collapsible ? (
        <button
          type="button"
          onClick={handleToggle}
          className={`${titleClass} hover:text-neutral-900 dark:hover:text-neutral-50`}
          aria-expanded={!hideBody}
          data-tauri-drag-region="false"
        >
          {streaming ? (
            <span className="reasoning-shimmer-text">Thinking…</span>
          ) : (
            <span>Thinking</span>
          )}
          <ChevronDown
            size={12}
            strokeWidth={2}
            className={`ml-auto shrink-0 transition-transform duration-300 ${open ? 'rotate-180' : ''}`}
          />
        </button>
      ) : (
        <div className={titleClass}>
          {streaming ? (
            <span className="reasoning-shimmer-text">Thinking…</span>
          ) : (
            <span>Thinking</span>
          )}
        </div>
      )}

      <div
        ref={bodyRef}
        className={bodyClass}
        aria-hidden={hideBody}
        style={bodyMaxHeight == null ? undefined : { maxHeight: `${bodyMaxHeight}px` }}
      >
        {collapsible && (
          <div data-testid="reasoning-frame" className="reasoning-scroll-frame">
            <div
              ref={scrollRef}
              data-testid="reasoning-scroll"
              className={scrollClass}
            >
              <div data-testid="reasoning-text" className="reasoning-plain-text">
                {visibleReasoning}
              </div>
            </div>
          </div>
        )}
      </div>
    </section>
  )
}
