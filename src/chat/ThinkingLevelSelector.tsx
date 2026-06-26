import { useState } from 'react'
import { Brain, Check, ChevronDown } from 'lucide-react'
import { chatTitlebarPillButtonClass } from './platform'
import type { ThinkingLevel } from './types'

interface ThinkingLevelSelectorProps {
  /** 当前等级；null = 跟随全局思考开关。 */
  value: ThinkingLevel | null
  onChange: (level: ThinkingLevel | null) => void
}

// null 用 '' 作 key，避免 Map 里 null 的歧义。
const OPTIONS: Array<{ value: ThinkingLevel | null; label: string }> = [
  { value: null, label: '跟随全局' },
  { value: 'off', label: '关闭' },
  { value: 'low', label: '低' },
  { value: 'medium', label: '中' },
  { value: 'high', label: '高' },
]

function labelFor(value: ThinkingLevel | null): string {
  return OPTIONS.find((o) => o.value === value)?.label ?? '跟随全局'
}

export function ThinkingLevelSelector({ value, onChange }: ThinkingLevelSelectorProps) {
  const [open, setOpen] = useState(false)
  // 跟随全局时只显示图标，避免占位；选了具体等级才显示文字。
  const showLabel = value != null

  return (
    <div className="relative max-w-full min-w-0" data-tauri-drag-region="false">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={`${chatTitlebarPillButtonClass} max-w-full min-w-0`}
        title={`思考等级：${labelFor(value)}`}
        aria-label={`思考等级：${labelFor(value)}`}
      >
        <Brain size={15} className="shrink-0 text-neutral-500 dark:text-neutral-400" />
        {showLabel && (
          <span className="max-w-[64px] truncate font-medium text-neutral-800 dark:text-neutral-200">
            {labelFor(value)}
          </span>
        )}
        <ChevronDown
          size={15}
          className={`shrink-0 text-neutral-400 transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} aria-hidden />
          <div className="chat-model-selector-menu chat-motion-popover absolute left-0 top-full z-20 mt-2 min-w-[160px] overflow-y-auto rounded-2xl border border-neutral-200/90 bg-white py-1 shadow-lg dark:border-neutral-700 dark:bg-neutral-900">
            {OPTIONS.map((opt) => {
              const active = opt.value === value
              return (
                <button
                  key={opt.value ?? ''}
                  type="button"
                  onClick={() => {
                    onChange(opt.value)
                    setOpen(false)
                  }}
                  className={`flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-[13px] transition-colors ${
                    active
                      ? 'bg-neutral-100 font-medium text-neutral-900 dark:bg-neutral-800 dark:text-neutral-100'
                      : 'text-neutral-700 hover:bg-neutral-50 dark:text-neutral-300 dark:hover:bg-neutral-800/80'
                  }`}
                >
                  <span className="min-w-0 truncate">{opt.label}</span>
                  {active && <Check size={15} className="shrink-0 text-neutral-500" />}
                </button>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}
