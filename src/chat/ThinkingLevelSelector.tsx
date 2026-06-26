import { useEffect, useMemo, useState } from 'react'
import { Brain, Check, ChevronDown } from 'lucide-react'
import { api } from '../api/tauri'
import { isProviderEnabled } from '../settings/utils'
import { chatTitlebarPillButtonClass } from './platform'
import type { ThinkingLevel } from './types'

interface ThinkingLevelSelectorProps {
  /** 当前等级；null = 跟随全局思考开关。 */
  value: ThinkingLevel | null
  currentProviderId: string
  currentModel: string
  onChange: (level: ThinkingLevel | null) => void
}

// 固定项 + 各等级的中文标签。具体显示哪些等级由后端按模型库决定。
const LABELS: Record<string, string> = {
  off: '关闭',
  low: '低',
  medium: '中',
  high: '高',
  xhigh: '超高',
  max: '最高',
}
const FOLLOW = '跟随全局'
// 未取到模型能力时的安全兜底（全模型通用子集）。
const FALLBACK_LEVELS = ['low', 'medium', 'high']

function labelFor(value: ThinkingLevel | null): string {
  return value == null ? FOLLOW : LABELS[value] ?? value
}

export function ThinkingLevelSelector({
  value,
  currentProviderId,
  currentModel,
  onChange,
}: ThinkingLevelSelectorProps) {
  const [open, setOpen] = useState(false)
  const [levels, setLevels] = useState<string[]>(FALLBACK_LEVELS)

  // 思考等级清单来自后端模型库（reasoningEfforts），按 (model, apiFormat) 解析。
  useEffect(() => {
    let alive = true
    void (async () => {
      if (!currentModel) {
        if (alive) setLevels(FALLBACK_LEVELS)
        return
      }
      try {
        const settings = await api.getSettings()
        const apiFormat = (settings.providers || [])
          .filter(isProviderEnabled)
          .find((p) => p.id === currentProviderId)?.apiFormat
        const got = await api.reasoningEffortsForModel(currentModel, apiFormat)
        if (alive) setLevels(got.length > 0 ? got : FALLBACK_LEVELS)
      } catch {
        if (alive) setLevels(FALLBACK_LEVELS)
      }
    })()
    return () => {
      alive = false
    }
  }, [currentProviderId, currentModel])

  const options = useMemo<Array<{ value: ThinkingLevel | null; label: string }>>(
    () => [
      { value: null, label: FOLLOW },
      { value: 'off', label: LABELS.off },
      ...levels.map((l) => ({ value: l as ThinkingLevel, label: LABELS[l] ?? l })),
    ],
    [levels],
  )

  // 跟随全局时只显示图标；选了具体等级才显示文字。
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
            {options.map((opt) => {
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
