import { invoke } from '@tauri-apps/api/core'
import type { Attachment, PendingAttachment } from './types'

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

type AttachmentLike = Pick<Attachment, 'path' | 'name' | 'type'>

function isLocalAttachmentPath(path: string): boolean {
  return path.includes('/') || path.includes('\\')
}

export async function loadAttachmentDataUrl(
  attachment: AttachmentLike,
  conversationId?: string | null,
): Promise<string | null> {
  if (!isTauriRuntime() || attachment.type !== 'image') return null
  const previewConversationId = isLocalAttachmentPath(attachment.path) ? null : conversationId
  try {
    const result = await invoke<{ success: boolean; data?: string; error?: string }>(
      'chat_read_attachment',
      {
        conversationId: previewConversationId ?? null,
        path: attachment.path,
      },
    )
    if (!result.success || !result.data) {
      console.warn('Failed to load attachment preview:', result.error ?? attachment.name)
      return null
    }
    return result.data
  } catch (err) {
    console.warn('Failed to load attachment preview:', err)
    return null
  }
}

export async function openAttachment(
  attachment: AttachmentLike,
  conversationId?: string | null,
): Promise<void> {
  if (!isTauriRuntime()) return
  await invoke('chat_open_attachment', {
    conversationId: conversationId ?? null,
    path: attachment.path,
  })
}

export type DisplayAttachment = Attachment | PendingAttachment
