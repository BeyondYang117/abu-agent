import { useMemo, useState } from 'react'
import { Check, ChevronDown, Eye, Sparkles, X } from 'lucide-react'
import type { SkillMeta } from './types'

export interface SkillSelectorProps {
  skills: SkillMeta[]
  value?: string | null
  onChange: (skillId: string | null, skill?: SkillMeta) => void
  disabled?: boolean
  loading?: boolean
  placeholder?: string
  emptyLabel?: string
  clearLabel?: string
  variant?: 'titlebar' | 'input'
  align?: 'left' | 'right'
  className?: string
  onPreviewSkill?: (skill: SkillMeta) => void
}

function recommendedTools(skill: SkillMeta): string[] {
  return skill.recommended_tools ?? skill.recommendedTools ?? []
}

function sourceLabel(skill: SkillMeta): string {
  if (!skill.source) return ''
  if (skill.source === 'builtin') return '内置'
  if (skill.source === 'user') return '用户'
  if (skill.source === 'external') return '外部'
  return skill.source
}

export function SkillSelector({
  skills,
  value,
  onChange,
  disabled = false,
  loading = false,
  placeholder = 'Skill',
  emptyLabel = '暂无 Skill',
  clearLabel = '清除 Skill',
  variant = 'titlebar',
  align = 'left',
  className = '',
  onPreviewSkill,
}: SkillSelectorProps) {
  const [open, setOpen] = useState(false)
  const selectedSkill = useMemo(
    () => skills.find((skill) => skill.id === value),
    [skills, value],
  )

  const triggerClass =
    variant === 'input'
      ? 'rounded-xl border border-neutral-200/90 bg-white px-2.5 py-2 text-sm dark:border-neutral-700 dark:bg-neutral-900'
      : 'rounded-full border border-neutral-200/90 bg-white px-3 py-1.5 text-sm shadow-sm dark:border-neutral-700 dark:bg-neutral-900'

  return (
    <div className={`relative ${className}`} data-tauri-drag-region="false">
      <div className={`inline-flex max-w-full items-center gap-1.5 transition-colors hover:bg-neutral-50 dark:hover:bg-neutral-800 ${triggerClass}`}>
        <button
          type="button"
          disabled={disabled}
          onClick={() => setOpen((isOpen) => !isOpen)}
          className="inline-flex min-w-0 items-center gap-1.5 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Sparkles size={14} strokeWidth={1.9} className="shrink-0 text-[#C56646] dark:text-[#E39A78]" />
          <span className="min-w-0 max-w-[180px] truncate font-medium text-neutral-800 dark:text-neutral-200">
            {selectedSkill?.name || (loading ? '加载中…' : placeholder)}
          </span>
        </button>
        {selectedSkill && (
          <button
            type="button"
            disabled={disabled}
            title={clearLabel}
            aria-label={clearLabel}
            onClick={(event) => {
              event.stopPropagation()
              onChange(null)
              setOpen(false)
            }}
            className="rounded-full p-0.5 text-neutral-400 transition-colors hover:bg-neutral-100 hover:text-neutral-700 disabled:cursor-not-allowed disabled:opacity-50 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
          >
            <X size={12} strokeWidth={2} />
          </button>
        )}
        <button
          type="button"
          disabled={disabled}
          onClick={() => setOpen((isOpen) => !isOpen)}
          className="inline-flex disabled:cursor-not-allowed disabled:opacity-50"
          aria-label={selectedSkill ? selectedSkill.name : placeholder}
        >
          <ChevronDown
            size={15}
            className={`shrink-0 text-neutral-400 transition-transform duration-[var(--kv-dur-fast)] ease-[var(--kv-ease-standard)] ${open ? 'rotate-180' : ''}`}
          />
        </button>
      </div>

      {open && !disabled && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} aria-hidden />
          <div
            className={`chat-motion-popover chat-popover-scroll absolute top-full z-20 mt-2 max-h-[min(420px,60vh)] w-[min(360px,calc(100vw-32px))] overflow-y-auto rounded-2xl border border-neutral-200/90 bg-white p-1 shadow-lg dark:border-neutral-700 dark:bg-neutral-900 ${
              align === 'right' ? 'right-0' : 'left-0'
            }`}
            style={{ ['--chat-popover-origin' as string]: align === 'right' ? 'top right' : 'top left' }}
          >
            {skills.map((skill) => {
              const active = selectedSkill?.id === skill.id
              const tools = recommendedTools(skill)
              const source = sourceLabel(skill)

              return (
                <button
                  key={skill.id}
                  type="button"
                  onClick={() => {
                    onChange(skill.id, skill)
                    setOpen(false)
                  }}
                  onDoubleClick={() => onPreviewSkill?.(skill)}
                  className={`group w-full rounded-xl px-3 py-2.5 text-left transition-colors ${
                    active
                      ? 'bg-neutral-100 text-neutral-900 dark:bg-neutral-800 dark:text-neutral-100'
                      : 'text-neutral-700 hover:bg-neutral-50 dark:text-neutral-300 dark:hover:bg-neutral-800/80'
                  }`}
                >
                  <div className="flex min-w-0 items-start gap-2">
                    <Sparkles
                      size={14}
                      strokeWidth={1.8}
                      className={`mt-0.5 shrink-0 ${
                        active
                          ? 'text-[#C56646] dark:text-[#E39A78]'
                          : 'text-neutral-400 group-hover:text-neutral-500 dark:group-hover:text-neutral-300'
                      }`}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex min-w-0 items-center gap-1.5">
                        <span className="min-w-0 truncate text-[13px] font-medium">
                          {skill.name}
                        </span>
                        {source && (
                          <span className="shrink-0 rounded bg-neutral-100 px-1.5 py-0.5 text-[10.5px] text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
                            {source}
                          </span>
                        )}
                      </div>
                      {skill.description && (
                        <div className="mt-0.5 line-clamp-2 text-[11.5px] leading-4 text-neutral-500 dark:text-neutral-400">
                          {skill.description}
                        </div>
                      )}
                      {tools.length > 0 && (
                        <div className="mt-1 flex flex-wrap gap-1">
                          {tools.slice(0, 3).map((tool) => (
                            <span
                              key={tool}
                              className="rounded bg-neutral-100 px-1.5 py-0.5 text-[10.5px] text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400"
                            >
                              {tool}
                            </span>
                          ))}
                          {tools.length > 3 && (
                            <span className="rounded bg-neutral-100 px-1.5 py-0.5 text-[10.5px] text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
                              +{tools.length - 3}
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                    <div className="mt-0.5 flex shrink-0 items-center gap-1">
                      {onPreviewSkill && (
                        <span
                          role="button"
                          tabIndex={0}
                          title="预览 Skill"
                          aria-label={`预览 ${skill.name}`}
                          onClick={(event) => {
                            event.stopPropagation()
                            onPreviewSkill(skill)
                            setOpen(false)
                          }}
                          onKeyDown={(event) => {
                            if (event.key !== 'Enter' && event.key !== ' ') return
                            event.preventDefault()
                            event.stopPropagation()
                            onPreviewSkill(skill)
                            setOpen(false)
                          }}
                          className="rounded-md p-1 text-neutral-400 opacity-0 transition group-hover:opacity-100 hover:bg-neutral-100 hover:text-neutral-700 dark:hover:bg-neutral-700 dark:hover:text-neutral-100"
                        >
                          <Eye size={13} strokeWidth={1.9} />
                        </span>
                      )}
                      {active && (
                        <Check
                          size={14}
                          strokeWidth={2}
                          className="text-[#C56646] dark:text-[#E39A78]"
                        />
                      )}
                    </div>
                  </div>
                </button>
              )
            })}

            {!loading && skills.length === 0 && (
              <div className="px-4 py-6 text-center text-sm text-neutral-500 dark:text-neutral-400">
                {emptyLabel}
              </div>
            )}
            {loading && (
              <div className="px-4 py-6 text-center text-sm text-neutral-500 dark:text-neutral-400">
                加载中…
              </div>
            )}
          </div>
        </>
      )}
    </div>
  )
}
