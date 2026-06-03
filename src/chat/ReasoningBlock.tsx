import { useEffect, useMemo, useState } from 'react'
import { ChevronDown } from 'lucide-react'

const COLLAPSE_LINE_LIMIT = 3
const CHARS_PER_LINE = 60

function reasoningExceedsLineLimit(text: string): boolean {
  const trimmed = text.trim()
  if (!trimmed) return false
  if (trimmed.split(/\r?\n/).length > COLLAPSE_LINE_LIMIT) return true
  return Math.ceil(trimmed.length / CHARS_PER_LINE) > COLLAPSE_LINE_LIMIT
}

type ReasoningBlockProps = {
  reasoning: string
  active?: boolean
}

export function ReasoningBlock({ reasoning, active = false }: ReasoningBlockProps) {
  const collapsible = useMemo(() => reasoningExceedsLineLimit(reasoning), [reasoning])
  const [open, setOpen] = useState(() => !collapsible)

  useEffect(() => {
    if (active) {
      setOpen(true)
      return
    }
    setOpen(!collapsible)
  }, [active, collapsible, reasoning])

  const titleClass = 'mb-1 text-[11px] font-medium text-neutral-400 dark:text-neutral-500'
  const bodyClass = `whitespace-pre-wrap leading-relaxed opacity-90 text-sm text-neutral-400 dark:text-neutral-500 ${
    collapsible && !open ? 'line-clamp-3' : ''
  } ${collapsible && open ? 'max-h-[200px] overflow-y-auto custom-scrollbar' : ''}`

  return (
    <section
      aria-label="思考过程"
      className="mb-3 border-l border-neutral-200 pl-3 dark:border-neutral-700"
    >
      {collapsible ? (
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          className={`${titleClass} flex w-full items-center gap-1 text-left hover:text-neutral-600 dark:hover:text-neutral-300`}
          aria-expanded={open}
          data-tauri-drag-region="false"
        >
          <span>思考过程</span>
          <ChevronDown
            size={12}
            strokeWidth={2}
            className={`shrink-0 transition-transform ${open ? 'rotate-180' : ''}`}
          />
        </button>
      ) : (
        <div className={titleClass}>思考过程</div>
      )}
      <div className={bodyClass}>{reasoning}</div>
    </section>
  )
}
