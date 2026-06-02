import { useEffect, useRef, useState } from 'react'
import { ArrowUp, Plus, SlidersHorizontal } from 'lucide-react'

interface InputBarProps {
  onSend: (content: string) => void
  disabled?: boolean
  onOpenSettings?: () => void
  autoFocus?: boolean
  /** footer：贴底（有消息时）；inline：嵌入居中区域（空对话欢迎页） */
  layout?: 'footer' | 'inline'
}

export function InputBar({
  onSend,
  disabled,
  onOpenSettings,
  autoFocus,
  layout = 'footer',
}: InputBarProps) {
  const [input, setInput] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const handleSend = () => {
    const trimmed = input.trim()
    if (!trimmed || disabled) return
    onSend(trimmed)
    setInput('')
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const handleInput = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value)
    const el = e.target
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`
  }

  useEffect(() => {
    if (autoFocus) textareaRef.current?.focus()
  }, [autoFocus])

  const canSend = Boolean(input.trim()) && !disabled

  const wrapperClass =
    layout === 'inline'
      ? 'w-full'
      : 'shrink-0 px-6 pb-8 pt-2'

  const innerClass = layout === 'inline' ? 'w-full' : 'mx-auto w-full max-w-3xl'

  return (
    <div className={wrapperClass}>
      <div className={innerClass}>
        <div className="flex items-end gap-2 rounded-[28px] border border-neutral-200/90 bg-white px-3 py-2.5 shadow-[0_2px_12px_rgba(0,0,0,0.06)] dark:border-neutral-700 dark:bg-neutral-900 dark:shadow-none">
          <button
            type="button"
            disabled={disabled}
            className="mb-0.5 shrink-0 rounded-full p-2 text-neutral-500 transition-colors hover:bg-neutral-100 disabled:opacity-40 dark:hover:bg-neutral-800"
            title="添加附件"
            aria-label="添加附件"
          >
            <Plus size={20} strokeWidth={1.75} />
          </button>

          {onOpenSettings && (
            <button
              type="button"
              onClick={onOpenSettings}
              disabled={disabled}
              className="mb-0.5 shrink-0 rounded-full p-2 text-neutral-500 transition-colors hover:bg-neutral-100 disabled:opacity-40 dark:hover:bg-neutral-800"
              title="设置"
              aria-label="设置"
            >
              <SlidersHorizontal size={18} strokeWidth={1.75} />
            </button>
          )}

          <textarea
            ref={textareaRef}
            value={input}
            onChange={handleInput}
            onKeyDown={handleKeyDown}
            disabled={disabled}
            placeholder="随便问我什么..."
            rows={1}
            className="mb-0.5 max-h-40 min-h-[28px] flex-1 resize-none border-0 bg-transparent px-1 py-1.5 text-[15px] leading-relaxed text-neutral-900 outline-none placeholder:text-neutral-400 disabled:opacity-50 dark:text-neutral-100"
          />

          <button
            type="button"
            onClick={handleSend}
            disabled={!canSend}
            className={`mb-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-full transition-all ${
              canSend
                ? 'bg-[#e8a090] text-white shadow-sm hover:bg-[#df9585]'
                : 'bg-neutral-200 text-neutral-400 dark:bg-neutral-700 dark:text-neutral-500'
            }`}
            title="发送"
            aria-label="发送"
          >
            <ArrowUp size={18} strokeWidth={2.25} />
          </button>
        </div>
      </div>
    </div>
  )
}
