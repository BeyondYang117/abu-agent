import { useEffect, useState } from 'react'
import { AssistantMessageMeta } from './AssistantMessageMeta'
import type { ChatMessage } from './types'

interface MessageBubbleProps {
  message: ChatMessage
  tokensPerSec?: number
  onUpdateMessage?: (messageId: string, content: string) => Promise<void>
  onRegenerateMessage?: (messageId: string) => Promise<void>
  onDeleteMessage?: (messageId: string) => Promise<void>
}

export function MessageBubble({
  message,
  tokensPerSec,
  onUpdateMessage,
  onRegenerateMessage,
  onDeleteMessage,
}: MessageBubbleProps) {
  const isUser = message.role === 'user'
  const canMutate = Boolean(onUpdateMessage && onDeleteMessage && onRegenerateMessage)
  const [isEditing, setIsEditing] = useState(false)
  const [draft, setDraft] = useState(message.content)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    setDraft(message.content)
    setIsEditing(false)
  }, [message.id, message.content])

  if (isUser) {
    return (
      <div className="flex justify-end py-2">
        <div className="max-w-[85%] rounded-[20px] bg-neutral-100 px-4 py-2.5 text-neutral-900 dark:bg-neutral-800 dark:text-neutral-100">
          <div className="whitespace-pre-wrap break-words text-[15px] leading-relaxed">
            {message.content}
          </div>
          {message.attachments && message.attachments.length > 0 && (
            <div className="mt-2 space-y-2">
              {message.attachments.map((att) => (
                <div
                  key={att.id}
                  className="flex items-center gap-2 rounded-lg bg-black/5 p-2 text-sm dark:bg-white/10"
                >
                  {att.type === 'image' ? '🖼️' : '📎'} {att.name}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    )
  }

  const handleSaveEdit = async () => {
    const trimmed = draft.trim()
    if (!trimmed || !onUpdateMessage) return
    setSaving(true)
    try {
      await onUpdateMessage(message.id, trimmed)
      setIsEditing(false)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex justify-start py-3">
      <div className="max-w-[85%] min-w-0">
        {message.reasoning && !isEditing && (
          <div className="mb-2 text-sm text-neutral-400 dark:text-neutral-500">
            <div className="mb-1">思考过程</div>
            <div className="whitespace-pre-wrap leading-relaxed opacity-90">
              {message.reasoning}
            </div>
          </div>
        )}

        {isEditing ? (
          <div className="space-y-2">
            <textarea
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              rows={6}
              disabled={saving}
              className="w-full resize-y rounded-xl border border-neutral-200/90 bg-white px-3 py-2.5 text-[15px] leading-relaxed text-neutral-900 outline-none focus:border-neutral-400 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100 dark:focus:border-neutral-500"
            />
            <div className="flex items-center gap-2">
              <button
                type="button"
                disabled={saving || !draft.trim()}
                onClick={() => void handleSaveEdit()}
                className="rounded-lg bg-neutral-900 px-3 py-1.5 text-sm font-medium text-white disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900"
              >
                {saving ? '保存中…' : '保存'}
              </button>
              <button
                type="button"
                disabled={saving}
                onClick={() => {
                  setDraft(message.content)
                  setIsEditing(false)
                }}
                className="rounded-lg px-3 py-1.5 text-sm text-neutral-600 hover:bg-neutral-100 disabled:opacity-40 dark:text-neutral-400 dark:hover:bg-neutral-800"
              >
                取消
              </button>
            </div>
          </div>
        ) : (
          <div className="whitespace-pre-wrap break-words text-[15px] leading-[1.7] text-neutral-900 dark:text-neutral-100">
            {message.content}
          </div>
        )}

        {!isEditing && message.content.trim().length > 0 && (
          <AssistantMessageMeta
            content={message.content}
            reasoning={message.reasoning}
            timestamp={message.timestamp}
            tokensPerSec={tokensPerSec}
            onEdit={canMutate ? () => setIsEditing(true) : undefined}
            onRegenerate={
              canMutate
                ? () => {
                    void onRegenerateMessage!(message.id)
                  }
                : undefined
            }
            onDelete={
              canMutate
                ? () => {
                    void onDeleteMessage!(message.id)
                  }
                : undefined
            }
          />
        )}

        {message.attachments && message.attachments.length > 0 && (
          <div className="mt-2 space-y-2">
            {message.attachments.map((att) => (
              <div
                key={att.id}
                className="flex items-center gap-2 rounded-lg border border-neutral-200/80 p-2 text-sm dark:border-neutral-700"
              >
                {att.type === 'image' ? '🖼️' : '📎'} {att.name}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
