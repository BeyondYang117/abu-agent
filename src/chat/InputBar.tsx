import { useCallback, useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import {
  ArrowUp,
  Check,
  Eye,
  FileText,
  Image,
  Plus,
  Settings,
  SlidersHorizontal,
  Sparkles,
  Square,
  Wrench,
  X,
} from 'lucide-react'
import type { ChatToolDefinition } from '../api/tauri'
import type { PendingAttachment, SkillMeta } from './types'

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'tiff', 'tif', 'heic', 'heif']
const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

interface InputBarProps {
  onSend: (content: string, attachments: PendingAttachment[]) => void
  disabled?: boolean
  onCancel?: () => void
  cancelVisible?: boolean
  cancelling?: boolean
  onOpenSettings?: () => void
  toolCount?: number
  enabledTools?: ChatToolDefinition[]
  toolsRequested?: boolean
  toolsDisabledReason?: string
  toolStatusHint?: string
  sendDisabledReason?: string
  skills?: SkillMeta[]
  activeSkillId?: string | null
  skillsLoading?: boolean
  onSkillChange?: (skillId: string | null, skill?: SkillMeta) => void
  onPreviewSkill?: (skill: SkillMeta) => void
  autoFocus?: boolean
  /** footer：贴底（有消息时）；inline：嵌入居中区域（空对话欢迎页） */
  layout?: 'footer' | 'inline'
}

function recommendedTools(skill?: SkillMeta | null): string[] {
  return skill?.recommended_tools ?? skill?.recommendedTools ?? []
}

function sourceLabel(skill: SkillMeta): string {
  if (!skill.source) return ''
  if (skill.source === 'builtin') return '内置'
  if (skill.source === 'user') return '用户'
  if (skill.source === 'external') return '外部'
  return skill.source
}

export function InputBar({
  onSend,
  disabled,
  onCancel,
  cancelVisible,
  cancelling,
  onOpenSettings,
  toolCount,
  enabledTools = [],
  toolsRequested,
  toolsDisabledReason,
  toolStatusHint,
  sendDisabledReason,
  skills = [],
  activeSkillId,
  skillsLoading = false,
  onSkillChange,
  onPreviewSkill,
  autoFocus,
  layout = 'footer',
}: InputBarProps) {
  const [input, setInput] = useState('')
  const [attachments, setAttachments] = useState<PendingAttachment[]>([])
  const [attachmentError, setAttachmentError] = useState('')
  const [dragActive, setDragActive] = useState(false)
  const [toolPanelOpen, setToolPanelOpen] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const attachmentsFromPaths = useCallback(
    (paths: string[]) =>
      paths.map((path) => {
        const normalized = path.replace(/\\/g, '/')
        const name = normalized.split('/').filter(Boolean).pop() || '附件'
        const ext = name.split('.').pop()?.toLowerCase() ?? ''
        const type: PendingAttachment['type'] = IMAGE_EXTENSIONS.includes(ext) ? 'image' : 'file'
        return {
          id: `pending-att-${crypto.randomUUID()}`,
          type,
          name,
          path,
        }
      }),
    [],
  )

  const addAttachments = useCallback(
    (next: PendingAttachment[], options?: { imagesOnly?: boolean }) => {
      const filtered = options?.imagesOnly
        ? next.filter((attachment) => attachment.type === 'image')
        : next
      if (filtered.length === 0) {
        setAttachmentError(options?.imagesOnly ? '请拖入图片文件' : '没有可添加的文件')
        return
      }

      setAttachments((prev) => {
        const existing = new Set(prev.map((attachment) => attachment.path))
        const dedupedNext = filtered.filter((attachment) => {
          if (existing.has(attachment.path)) return false
          existing.add(attachment.path)
          return true
        })
        if (dedupedNext.length === 0) {
          setAttachmentError('图片已添加')
          return prev
        }
        setAttachmentError('')
        return [...prev, ...dedupedNext]
      })
      textareaRef.current?.focus()
    },
    [],
  )

  const handleSend = () => {
    const trimmed = input.trim()
    if ((!trimmed && attachments.length === 0) || disabled || sendDisabledReason) return
    onSend(trimmed, attachments)
    setInput('')
    setAttachments([])
    setAttachmentError('')
    setToolPanelOpen(false)
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

  const handleAddAttachment = async () => {
    if (disabled) return
    setToolPanelOpen(false)
    setAttachmentError('')
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [
          {
            name: '图片',
            extensions: IMAGE_EXTENSIONS,
          },
        ],
      })
      const paths = Array.isArray(selected) ? selected : selected ? [selected] : []
      if (paths.length === 0) return

      addAttachments(attachmentsFromPaths(paths))
    } catch (err) {
      console.error('Failed to add chat attachment:', err)
      setAttachmentError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : '添加附件失败',
      )
    }
  }

  const removeAttachment = (id: string) => {
    setAttachments((prev) => prev.filter((attachment) => attachment.id !== id))
    setAttachmentError('')
  }

  useEffect(() => {
    if (autoFocus) textareaRef.current?.focus()
  }, [autoFocus])

  useEffect(() => {
    if (!toolPanelOpen) return
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setToolPanelOpen(false)
      }
    }
    window.addEventListener('keydown', handleEscape)
    return () => window.removeEventListener('keydown', handleEscape)
  }, [toolPanelOpen])

  useEffect(() => {
    if (disabled) setToolPanelOpen(false)
  }, [disabled])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let cancelled = false
    let unlisten: (() => void) | undefined

    getCurrentWebview().onDragDropEvent((event) => {
      if (cancelled || disabled) return

      if (event.payload.type === 'enter' || event.payload.type === 'over') {
        setDragActive(true)
        setAttachmentError('')
        return
      }

      if (event.payload.type === 'leave') {
        setDragActive(false)
        return
      }

      if (event.payload.type === 'drop') {
        setDragActive(false)
        addAttachments(attachmentsFromPaths(event.payload.paths), { imagesOnly: true })
      }
    }).then((handler) => {
      if (cancelled) {
        handler()
      } else {
        unlisten = handler
      }
    }).catch((err) => {
      console.error('Failed to listen for chat image drops:', err)
    })

    return () => {
      cancelled = true
      setDragActive(false)
      unlisten?.()
    }
  }, [addAttachments, attachmentsFromPaths, disabled])

  const canSend = (Boolean(input.trim()) || attachments.length > 0) && !disabled && !sendDisabledReason

  const wrapperClass =
    layout === 'inline'
      ? 'w-full'
      : 'shrink-0 px-6 pb-8 pt-2'

  const innerClass = layout === 'inline' ? 'w-full' : 'mx-auto w-full max-w-3xl'
  const activeSkill = skills.find((skill) => skill.id === activeSkillId) ?? null
  const activeSkillTools = recommendedTools(activeSkill)
  const hasToolProblem = Boolean(toolsDisabledReason || toolStatusHint || sendDisabledReason)
  const toolSummary = toolsDisabledReason
    || (toolCount !== undefined && toolCount > 0 ? `已启用 ${toolCount} 个工具` : '未启用工具')

  return (
    <div className={wrapperClass}>
      <div className={`relative ${innerClass}`}>
        {toolPanelOpen && (
          <>
            <div className="fixed inset-0 z-30" onClick={() => setToolPanelOpen(false)} aria-hidden />
            <div
              className="absolute bottom-full left-10 z-40 mb-2 w-[min(320px,calc(100vw-32px))] overflow-hidden rounded-xl border border-neutral-200/90 bg-white shadow-[0_10px_28px_rgba(0,0,0,0.14)] dark:border-neutral-700 dark:bg-neutral-900"
              data-tauri-drag-region="false"
            >
              <div className="flex items-center gap-2 border-b border-neutral-200/80 px-3 py-2 dark:border-neutral-800">
                <div className="flex min-w-0 flex-1 items-center gap-2">
                  <SlidersHorizontal size={15} strokeWidth={1.8} className="shrink-0 text-neutral-500 dark:text-neutral-400" />
                  <div className="truncate text-[13px] font-semibold text-neutral-900 dark:text-neutral-100">
                    MCP / Skill
                  </div>
                </div>
                {onOpenSettings && (
                  <button
                    type="button"
                    onClick={() => {
                      setToolPanelOpen(false)
                      onOpenSettings()
                    }}
                    className="rounded-md p-1 text-neutral-500 transition-colors hover:bg-neutral-100 hover:text-neutral-800 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
                    title="配置 MCP 与 Skill"
                    aria-label="配置 MCP 与 Skill"
                  >
                    <Settings size={15} strokeWidth={1.9} />
                  </button>
                )}
              </div>

              <div className="max-h-[min(360px,58vh)] overflow-y-auto p-2">
                <section>
                  <div className="mb-1 flex items-center justify-between gap-2 px-1">
                    <div className="flex min-w-0 items-center gap-1.5 text-[12px] font-semibold text-neutral-700 dark:text-neutral-200">
                      <Sparkles size={13} strokeWidth={1.9} className="shrink-0 text-[#C56646] dark:text-[#E39A78]" />
                      <span>Skill</span>
                    </div>
                    {activeSkill && onSkillChange && (
                      <button
                        type="button"
                        onClick={() => {
                          onSkillChange(null)
                          textareaRef.current?.focus()
                        }}
                        className="rounded-md px-2 py-1 text-[11px] text-neutral-500 transition-colors hover:bg-neutral-100 hover:text-neutral-800 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
                      >
                        清除
                      </button>
                    )}
                  </div>

                  {skillsLoading && (
                    <div className="px-2 py-3 text-center text-[12px] text-neutral-500 dark:text-neutral-400">
                      加载中…
                    </div>
                  )}

                  {!skillsLoading && skills.length === 0 && (
                    <div className="px-2 py-3 text-center text-[12px] text-neutral-500 dark:text-neutral-400">
                      暂无 Skill
                    </div>
                  )}

                  {!skillsLoading && skills.length > 0 && (
                    <div className="space-y-1">
                      {skills.map((skill) => {
                        const active = activeSkill?.id === skill.id
                        const tools = recommendedTools(skill)
                        const source = sourceLabel(skill)
                        return (
                          <button
                            key={skill.id}
                            type="button"
                            onClick={() => {
                              onSkillChange?.(active ? null : skill.id, active ? undefined : skill)
                              setToolPanelOpen(false)
                              textareaRef.current?.focus()
                            }}
                            title={skill.description || skill.name}
                            className={`group w-full rounded-lg px-2 py-1.5 text-left transition-colors ${
                              active
                                ? 'bg-neutral-100 text-neutral-900 dark:bg-neutral-800 dark:text-neutral-100'
                                : 'text-neutral-700 hover:bg-neutral-50 dark:text-neutral-300 dark:hover:bg-neutral-800/80'
                            }`}
                          >
                            <div className="flex min-w-0 items-start gap-2">
                              <Sparkles
                                size={13}
                                strokeWidth={1.8}
                                className={`mt-[3px] shrink-0 ${
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
                                    <span className="shrink-0 rounded bg-neutral-100 px-1.5 py-0.5 text-[10px] text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
                                      {source}
                                    </span>
                                  )}
                                </div>
                                {tools.length > 0 && (
                                  <div className="mt-0.5 flex flex-wrap gap-1">
                                    {tools.slice(0, 2).map((tool) => (
                                      <span
                                        key={tool}
                                        className="rounded bg-neutral-100 px-1.5 py-0.5 text-[10px] text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400"
                                      >
                                        {tool}
                                      </span>
                                    ))}
                                    {tools.length > 2 && (
                                      <span className="rounded bg-neutral-100 px-1.5 py-0.5 text-[10px] text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
                                        +{tools.length - 2}
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
                                      setToolPanelOpen(false)
                                    }}
                                    onKeyDown={(event) => {
                                      if (event.key !== 'Enter' && event.key !== ' ') return
                                      event.preventDefault()
                                      event.stopPropagation()
                                      onPreviewSkill(skill)
                                      setToolPanelOpen(false)
                                    }}
                                    className="rounded-md p-1 text-neutral-400 opacity-0 transition group-hover:opacity-100 hover:bg-neutral-100 hover:text-neutral-700 dark:hover:bg-neutral-700 dark:hover:text-neutral-100"
                                  >
                                    <Eye size={13} strokeWidth={1.9} />
                                  </span>
                                )}
                                {active && (
                                  <Check size={14} strokeWidth={2} className="text-[#C56646] dark:text-[#E39A78]" />
                                )}
                              </div>
                            </div>
                          </button>
                        )
                      })}
                    </div>
                  )}
                </section>

                <section className="mt-2 border-t border-neutral-200/80 px-1 pt-2 dark:border-neutral-800">
                  <div className="mb-1.5 flex items-center justify-between gap-2">
                    <div className="flex min-w-0 items-center gap-1.5 text-[12px] font-semibold text-neutral-700 dark:text-neutral-200">
                      <Wrench size={13} strokeWidth={1.9} className="shrink-0 text-neutral-500 dark:text-neutral-400" />
                      <span>MCP</span>
                    </div>
                    <span className={`rounded-full px-2 py-0.5 text-[11px] ${
                      hasToolProblem
                        ? 'bg-amber-100 text-amber-700 dark:bg-amber-400/15 dark:text-amber-200'
                        : 'bg-neutral-100 text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400'
                    }`}>
                      {toolSummary}
                    </span>
                  </div>

                  {(sendDisabledReason || toolStatusHint) && (
                    <div className="mb-1.5 rounded-md bg-amber-50 px-2 py-1.5 text-[11px] leading-4 text-amber-700 dark:bg-amber-400/10 dark:text-amber-200">
                      {sendDisabledReason || toolStatusHint}
                    </div>
                  )}

                  {activeSkillTools.length > 0 && (
                    <div className="mb-1.5 flex flex-wrap gap-1">
                      {activeSkillTools.map((tool) => (
                        <span
                          key={tool}
                          className="rounded bg-neutral-100 px-1.5 py-0.5 text-[10px] text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400"
                        >
                          {tool}
                        </span>
                      ))}
                    </div>
                  )}

                  {enabledTools.length > 0 ? (
                    <div className="flex flex-wrap gap-1.5">
                      {enabledTools.slice(0, 8).map((tool) => (
                        <span
                          key={tool.id}
                          className="rounded-md border border-neutral-200/80 px-1.5 py-0.5 text-[10.5px] text-neutral-600 dark:border-neutral-700 dark:text-neutral-300"
                          title={tool.description}
                        >
                          {tool.name}
                        </span>
                      ))}
                      {enabledTools.length > 8 && (
                        <span className="rounded-md border border-neutral-200/80 px-1.5 py-0.5 text-[10.5px] text-neutral-500 dark:border-neutral-700 dark:text-neutral-400">
                          +{enabledTools.length - 8}
                        </span>
                      )}
                    </div>
                  ) : (
                    <div className="text-[12px] text-neutral-500 dark:text-neutral-400">
                      {toolsDisabledReason || (toolsRequested ? '未发现可用工具' : '未启用 MCP 工具')}
                    </div>
                  )}
                </section>
              </div>
            </div>
          </>
        )}
        <div
          className={`rounded-[28px] border bg-white px-3 py-2.5 shadow-[0_2px_12px_rgba(0,0,0,0.06)] transition-colors dark:bg-neutral-900 dark:shadow-none ${
            dragActive
              ? 'border-[#e8a090] ring-2 ring-[#e8a090]/25 dark:border-[#e8a090]'
              : 'border-neutral-200/90 dark:border-neutral-700'
          }`}
        >
          {dragActive && (
            <div className="mb-2 rounded-2xl border border-dashed border-[#e8a090]/70 bg-[#e8a090]/10 px-3 py-2 text-center text-[13px] font-medium text-[#a35f51] dark:text-[#f1b4a7]">
              松开即可添加图片
            </div>
          )}
          {attachments.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-1.5 px-1">
              {attachments.map((attachment) => {
                const Icon = attachment.type === 'image' ? Image : FileText
                return (
                  <div
                    key={attachment.id}
                    className="flex max-w-[220px] items-center gap-1.5 rounded-full border border-neutral-200/90 bg-neutral-50 px-2.5 py-1 text-[12px] text-neutral-700 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
                    title={attachment.name}
                  >
                    <Icon size={13} strokeWidth={1.8} className="shrink-0 text-neutral-500" />
                    <span className="min-w-0 truncate">{attachment.name}</span>
                    <button
                      type="button"
                      onClick={() => removeAttachment(attachment.id)}
                      disabled={disabled}
                      className="-mr-1 rounded-full p-0.5 text-neutral-400 transition-colors hover:bg-black/[0.06] hover:text-neutral-700 disabled:opacity-40 dark:hover:bg-white/[0.08] dark:hover:text-neutral-100"
                      aria-label={`移除附件 ${attachment.name}`}
                    >
                      <X size={12} strokeWidth={2} />
                    </button>
                  </div>
                )
              })}
            </div>
          )}
          {attachmentError && (
            <div className="mb-2 px-1 text-[12px] text-red-500 dark:text-red-400">
              {attachmentError}
            </div>
          )}
          {(sendDisabledReason || toolStatusHint) && !attachmentError && (
            <div className="mb-2 px-1 text-[12px] text-amber-600 dark:text-amber-300">
              {sendDisabledReason || toolStatusHint}
            </div>
          )}
          <div className="flex items-end gap-2">
            <button
              type="button"
              onClick={() => void handleAddAttachment()}
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
                onClick={() => setToolPanelOpen((open) => !open)}
                disabled={disabled}
                className={`mb-0.5 shrink-0 rounded-full p-2 transition-colors disabled:opacity-40 ${
                  toolPanelOpen || activeSkill || hasToolProblem || (toolCount ?? 0) > 0
                    ? 'bg-neutral-100 text-neutral-800 dark:bg-neutral-800 dark:text-neutral-100'
                    : 'text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800'
                }`}
                title="MCP / Skill"
                aria-label="MCP / Skill"
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

            {cancelVisible && onCancel ? (
              <button
                type="button"
                onClick={onCancel}
                disabled={cancelling}
                className="mb-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-neutral-900 text-white shadow-sm transition-all hover:bg-neutral-700 disabled:bg-neutral-300 disabled:text-neutral-500 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-200 dark:disabled:bg-neutral-700 dark:disabled:text-neutral-500"
                title={cancelling ? '正在停止' : '停止生成'}
                aria-label={cancelling ? '正在停止' : '停止生成'}
              >
                <Square size={13} strokeWidth={2.4} fill="currentColor" />
              </button>
            ) : (
              <button
                type="button"
                onClick={handleSend}
                disabled={!canSend}
                title={sendDisabledReason || (canSend ? '发送' : '输入消息后发送')}
                aria-label={sendDisabledReason || '发送'}
                className={`mb-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-full transition-all ${
                  canSend
                    ? 'bg-[#e8a090] text-white shadow-sm hover:bg-[#df9585]'
                    : 'bg-neutral-200 text-neutral-400 dark:bg-neutral-700 dark:text-neutral-500'
                }`}
              >
                <ArrowUp size={18} strokeWidth={2.25} />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
