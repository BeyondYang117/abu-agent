import { memo, useEffect, useMemo, useState } from 'react'
import { Brain, Check, ChevronDown } from 'lucide-react'
import { api } from '../api/tauri'
import { useT } from '../settings/i18n'
import { chatTitlebarPillButtonClass } from './platform'
import { thinkingLevelDescription, thinkingLevelLabel } from './thinkingLevelLabels'
import type { ThinkingLevel } from './types'

interface ThinkingLevelSelectorProps {
  /** 当前等级；null = 自动，由当前模型采用推荐默认值。 */
  value: ThinkingLevel | null
  currentProviderId: string
  currentModel: string
  onChange: (level: ThinkingLevel | null) => void
}

// 未取到模型能力时的安全兜底（全模型通用子集）。
const FALLBACK_LEVELS = ['low', 'medium', 'high']

function ThinkingLevelSelectorBase({
  value,
  currentProviderId,
  currentModel,
  onChange,
}: ThinkingLevelSelectorProps) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [levels, setLevels] = useState<string[]>(FALLBACK_LEVELS)
  const [levelsLoaded, setLevelsLoaded] = useState(false)

  // 思考等级清单来自后端（用户在模型详情里的覆盖 → 模型库 reasoningEfforts → 家族兜底）。
  useEffect(() => {
    let alive = true
    setLevelsLoaded(false)
    void (async () => {
      if (!currentModel) {
        if (alive) {
          setLevels(FALLBACK_LEVELS)
          setLevelsLoaded(true)
        }
        return
      }
      try {
        const got = await api.reasoningEffortsForModel(currentModel, currentProviderId)
        // 空列表是有意义的答案（该模型没有 effort 旋钮），不能再兜底成 FALLBACK_LEVELS。
        if (alive) {
          setLevels(got)
          setLevelsLoaded(true)
        }
      } catch {
        if (alive) {
          setLevels(FALLBACK_LEVELS)
          setLevelsLoaded(true)
        }
      }
    })()
    return () => {
      alive = false
    }
  }, [currentProviderId, currentModel])

  // 存的手动档若不被新模型支持，收敛回自动，避免向 provider 下发非法值。
  const effective = useMemo<ThinkingLevel | null>(() => {
    if (value === null || value === 'off' || levels.length === 0 || levels.includes(value)) return value
    return null
  }, [value, levels])

  // 收敛结果要落盘，否则按钮显示自动、请求却仍按旧档位发出去。
  useEffect(() => {
    if (levelsLoaded && levels.length > 0 && effective !== value) onChange(effective)
  }, [effective, levelsLoaded, value, levels, onChange])

  const options = useMemo<Array<{ value: ThinkingLevel | null; label: string; description: string }>>(
    () => [
      {
        value: null,
        label: thinkingLevelLabel(null, t),
        description: thinkingLevelDescription(null, t),
      },
      {
        value: 'off',
        label: thinkingLevelLabel('off', t),
        description: thinkingLevelDescription('off', t),
      },
      ...levels.map((level) => ({
        value: level as ThinkingLevel,
        label: thinkingLevelLabel(level, t),
        description: thinkingLevelDescription(level as ThinkingLevel, t),
      })),
    ],
    [levels, t],
  )

  // 该模型没有思考等级可调（Claude 3.5 / GLM-4.7 / Kimi K2.x…）→ 不显示这个旋钮。
  if (levels.length === 0) return null

  return (
    <div className="relative max-w-full min-w-0" data-tauri-drag-region="false">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={`${chatTitlebarPillButtonClass} max-w-full min-w-0`}
        title={t.chatThinkingLevel.replace('{level}', thinkingLevelLabel(effective, t))}
        aria-label={t.chatThinkingLevel.replace('{level}', thinkingLevelLabel(effective, t))}
      >
        <Brain size={15} className="shrink-0 text-neutral-500 dark:text-neutral-400" />
        <span className="chat-thinking-level-label max-w-[88px] truncate font-medium text-neutral-800 dark:text-neutral-200">
          {thinkingLevelLabel(effective, t)}
        </span>
        <ChevronDown
          size={15}
          className={`shrink-0 text-neutral-400 transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} aria-hidden />
          <div className="chat-model-selector-menu chat-motion-popover absolute left-0 top-full z-20 mt-2 min-w-[240px] overflow-y-auto kv-menu">
            {options.map((opt) => {
              const active = opt.value === effective
              return (
                <button
                  key={opt.value ?? 'auto'}
                  type="button"
                  onClick={() => {
                    onChange(opt.value)
                    setOpen(false)
                  }}
                  className={`kv-menu-row items-start justify-between py-2 transition-colors ${
                    active
                      ? 'bg-neutral-100 font-medium text-neutral-900 dark:bg-neutral-800 dark:text-neutral-100'
                      : 'text-neutral-700 hover:bg-neutral-50 dark:text-neutral-300 dark:hover:bg-neutral-800/80'
                  }`}
                >
                  <span className="min-w-0">
                    <span className="block truncate">{opt.label}</span>
                    <span className="mt-0.5 block text-[11px] font-normal leading-4 text-neutral-400">
                      {opt.description}
                    </span>
                  </span>
                  {active && <Check size={15} className="mt-0.5 shrink-0 text-neutral-500" />}
                </button>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}

// memo：顶栏选择器，仅在 props 变化时重渲。
export const ThinkingLevelSelector = memo(ThinkingLevelSelectorBase)
