// Chat 前端类型定义

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  attachments?: Attachment[]
  reasoning?: string
  timestamp: number
}

export interface Attachment {
  id: string
  type: 'image' | 'file'
  name: string
  path: string
}

export interface Conversation {
  id: string
  title: string
  provider_id: string
  model: string
  messages: ChatMessage[]
  created_at: number
  updated_at: number
  pinned?: boolean
  folder?: string
}

export interface ConversationListItem {
  id: string
  title: string
  preview: string
  provider_id: string
  model: string
  message_count: number
  created_at: number
  updated_at: number
  pinned?: boolean
  folder?: string
}

export interface ConversationGroup {
  title: string
  conversations: ConversationListItem[]
}
