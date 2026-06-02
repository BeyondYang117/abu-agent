// Chat 工具函数
import type { ConversationListItem, ConversationGroup } from './types'

/**
 * 将对话列表按时间分组
 */
export function groupConversationsByTime(
  conversations: ConversationListItem[]
): ConversationGroup[] {
  const now = Date.now() / 1000
  const oneDayAgo = now - 86400
  const sevenDaysAgo = now - 86400 * 7
  const thirtyDaysAgo = now - 86400 * 30

  const groups: ConversationGroup[] = [
    { title: '今天', conversations: [] },
    { title: '昨天', conversations: [] },
    { title: '最近 7 天', conversations: [] },
    { title: '最近 30 天', conversations: [] },
    { title: '更早', conversations: [] },
  ]

  for (const conv of conversations) {
    if (conv.updated_at >= oneDayAgo) {
      groups[0].conversations.push(conv)
    } else if (conv.updated_at >= oneDayAgo - 86400) {
      groups[1].conversations.push(conv)
    } else if (conv.updated_at >= sevenDaysAgo) {
      groups[2].conversations.push(conv)
    } else if (conv.updated_at >= thirtyDaysAgo) {
      groups[3].conversations.push(conv)
    } else {
      groups[4].conversations.push(conv)
    }
  }

  // 过滤掉空分组
  return groups.filter((g) => g.conversations.length > 0)
}

/**
 * 格式化相对时间
 */
export function formatRelativeTime(timestamp: number): string {
  const now = Date.now() / 1000
  const diff = now - timestamp

  if (diff < 60) {
    return '刚刚'
  } else if (diff < 3600) {
    return `${Math.floor(diff / 60)} 分钟前`
  } else if (diff < 86400) {
    return `${Math.floor(diff / 3600)} 小时前`
  } else if (diff < 86400 * 7) {
    return `${Math.floor(diff / 86400)} 天前`
  } else {
    const date = new Date(timestamp * 1000)
    return date.toLocaleDateString('zh-CN', { month: 'long', day: 'numeric' })
  }
}

/**
 * 截断文本
 */
export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) {
    return text
  }
  return text.slice(0, maxLength) + '...'
}
