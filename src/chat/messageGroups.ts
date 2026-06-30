import type { ChatMessage } from './types'

// 多模型一问多答（任务 06-30）：把消息线性数组折叠成「单条消息 / 多答组」两类项。
// 同一 group_id 的连续 assistant 消息聚成一组（横向并排多列渲染）；其余保持线性。
// 纯函数，便于单测（grouping 边界 / 单模型零回归）。

export type MessageListItem =
  | { type: 'message'; message: ChatMessage }
  | { type: 'group'; groupId: string; messages: ChatMessage[] }

function messageGroupId(message: ChatMessage): string | null {
  return message.group_id ?? message.groupId ?? null
}

export function foldMessageGroups(messages: ChatMessage[]): MessageListItem[] {
  const items: MessageListItem[] = []
  for (let i = 0; i < messages.length; i++) {
    const message = messages[i]
    const groupId = messageGroupId(message)
    if (message.role === 'assistant' && groupId) {
      const groupMessages: ChatMessage[] = [message]
      let j = i + 1
      while (j < messages.length) {
        const next = messages[j]
        if (next.role === 'assistant' && messageGroupId(next) === groupId) {
          groupMessages.push(next)
          j++
        } else {
          break
        }
      }
      items.push({ type: 'group', groupId, messages: groupMessages })
      i = j - 1
      continue
    }
    items.push({ type: 'message', message })
  }
  return items
}
