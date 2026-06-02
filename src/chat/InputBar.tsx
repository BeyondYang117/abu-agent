import { useCallback, useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { ArrowUp, FileText, Image, Plus, SlidersHorizontal, X } from 'lucide-react'
import type { PendingAttachment } from './types'

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'tiff', 'tif', 'heic', 'heif']
const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

interface InputBarProps {
  onSend: (content: string, attachments: PendingAttachment[]) => void
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
  const [attachments, setAttachments] = useState<PendingAttachment[]>([])
  const [attachmentError, setAttachmentError] = useState('')
  const [dragActive, setDragActive] = useState(false)
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
    if ((!trimmed && attachments.length === 0) || disabled) return
    onSend(trimmed, attachments)
    setInput('')
    setAttachments([])
    setAttachmentError('')
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

  const canSend = (Boolean(input.trim()) || attachments.length > 0) && !disabled

  const wrapperClass =
    layout === 'inline'
      ? 'w-full'
      : 'shrink-0 px-6 pb-8 pt-2'

  const innerClass = layout === 'inline' ? 'w-full' : 'mx-auto w-full max-w-3xl'

  return (
    <div className={wrapperClass}>
      <div className={innerClass}>
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
    </div>
  )
}
