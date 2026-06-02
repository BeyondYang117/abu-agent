// Chat API 调用封装
import { invoke } from '@tauri-apps/api/core'
import type { Conversation, ConversationListItem } from './types'

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

const mockStorageKey = 'kivio-chat-dev-conversations'

const nowSeconds = () => Math.floor(Date.now() / 1000)

function loadMockConversations(): Conversation[] {
  try {
    const raw = window.localStorage.getItem(mockStorageKey)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed : []
  } catch {
    return []
  }
}

function saveMockConversations(conversations: Conversation[]) {
  window.localStorage.setItem(mockStorageKey, JSON.stringify(conversations))
}

function toListItem(conversation: Conversation): ConversationListItem {
  const preview = [...conversation.messages]
    .reverse()
    .find((message) => message.role === 'user' || message.role === 'assistant')
    ?.content.trim() ?? ''
  return {
    id: conversation.id,
    title: conversation.title,
    preview: preview.length > 100 ? `${preview.slice(0, 100)}...` : preview,
    provider_id: conversation.provider_id,
    model: conversation.model,
    message_count: conversation.messages.length,
    created_at: conversation.created_at,
    updated_at: conversation.updated_at,
    pinned: conversation.pinned,
    folder: conversation.folder,
  }
}

const mockChatApi = {
  async getConversations(offset = 0, limit = 50, folder?: string): Promise<ConversationListItem[]> {
    const conversations = loadMockConversations()
      .filter((conversation) => !folder || conversation.folder === folder)
      .sort((a, b) => b.updated_at - a.updated_at)
    return conversations.slice(offset, offset + limit).map(toListItem)
  },

  async getConversation(conversationId: string): Promise<Conversation> {
    const conversation = loadMockConversations().find((item) => item.id === conversationId)
    if (!conversation) throw new Error('Conversation not found')
    return conversation
  },

  async createConversation(providerId = 'dev-provider', model = 'dev-model', folder?: string): Promise<Conversation> {
    const now = nowSeconds()
    const conversation: Conversation = {
      id: `conv_dev_${crypto.randomUUID()}`,
      title: '新对话',
      provider_id: providerId,
      model,
      messages: [],
      created_at: now,
      updated_at: now,
      pinned: false,
      folder,
    }
    saveMockConversations([conversation, ...loadMockConversations()])
    return conversation
  },

  async sendMessage(conversationId: string, content: string): Promise<Conversation> {
    const conversations = loadMockConversations()
    const index = conversations.findIndex((item) => item.id === conversationId)
    if (index < 0) throw new Error('Conversation not found')
    const now = nowSeconds()
    const conversation = { ...conversations[index] }
    conversation.messages = [
      ...conversation.messages,
      {
        id: `msg_dev_${crypto.randomUUID()}`,
        role: 'user',
        content,
        timestamp: now,
      },
      {
        id: `msg_dev_${crypto.randomUUID()}`,
        role: 'assistant',
        content: '这是浏览器预览模式的本地回复。启动 Tauri 桌面应用后会调用真实模型接口。',
        timestamp: now,
      },
    ]
    if (conversation.title === '新对话') {
      conversation.title = content.length > 30 ? `${content.slice(0, 30)}...` : content
    }
    conversation.updated_at = now
    conversations[index] = conversation
    saveMockConversations(conversations)
    return conversation
  },

  async deleteConversation(conversationId: string): Promise<void> {
    saveMockConversations(loadMockConversations().filter((item) => item.id !== conversationId))
  },

  async updateConversation(
    conversationId: string,
    updates: {
      title?: string
      pinned?: boolean
      folder?: string
      providerId?: string
      model?: string
    }
  ): Promise<Conversation> {
    const conversations = loadMockConversations()
    const index = conversations.findIndex((item) => item.id === conversationId)
    if (index < 0) throw new Error('Conversation not found')
    const conversation = {
      ...conversations[index],
      title: updates.title ?? conversations[index].title,
      pinned: updates.pinned ?? conversations[index].pinned,
      folder: updates.folder ?? conversations[index].folder,
      provider_id: updates.providerId ?? conversations[index].provider_id,
      model: updates.model ?? conversations[index].model,
      updated_at: nowSeconds(),
    }
    conversations[index] = conversation
    saveMockConversations(conversations)
    return conversation
  },
}

export const chatApi = {
  // 获取对话列表
  async getConversations(
    offset = 0,
    limit = 50,
    folder?: string
  ): Promise<ConversationListItem[]> {
    if (!isTauriRuntime()) return mockChatApi.getConversations(offset, limit, folder)
    const result = await invoke<{ success: boolean; conversations: ConversationListItem[] }>(
      'chat_get_conversations',
      { offset, limit, folder }
    )
    if (!result.success) {
      throw new Error('Failed to get conversations')
    }
    return result.conversations
  },

  // 获取对话详情
  async getConversation(conversationId: string): Promise<Conversation> {
    if (!isTauriRuntime()) return mockChatApi.getConversation(conversationId)
    const result = await invoke<{ success: boolean; conversation: Conversation }>(
      'chat_get_conversation',
      { conversationId }
    )
    if (!result.success) {
      throw new Error('Failed to get conversation')
    }
    return result.conversation
  },

  // 创建新对话
  async createConversation(
    providerId?: string,
    model?: string,
    folder?: string
  ): Promise<Conversation> {
    if (!isTauriRuntime()) return mockChatApi.createConversation(providerId, model, folder)
    const result = await invoke<{ success: boolean; conversation: Conversation }>(
      'chat_create_conversation',
      { providerId, model, folder }
    )
    if (!result.success) {
      throw new Error('Failed to create conversation')
    }
    return result.conversation
  },

  // 发送消息
  async sendMessage(
    conversationId: string,
    content: string,
    attachments: string[] = []
  ): Promise<Conversation> {
    if (!isTauriRuntime()) return mockChatApi.sendMessage(conversationId, content)
    const result = await invoke<{ success: boolean; conversation?: Conversation; error?: string }>(
      'chat_send_message',
      { conversationId, content, attachments }
    )
    if (!result.success || !result.conversation) {
      throw new Error(result.error || 'Failed to send message')
    }
    return result.conversation
  },

  // 删除对话
  async deleteConversation(conversationId: string): Promise<void> {
    if (!isTauriRuntime()) return mockChatApi.deleteConversation(conversationId)
    const result = await invoke<{ success: boolean }>('chat_delete_conversation', {
      conversationId,
    })
    if (!result.success) {
      throw new Error('Failed to delete conversation')
    }
  },

  // 更新对话
  async updateConversation(
    conversationId: string,
    updates: {
      title?: string
      pinned?: boolean
      folder?: string
      providerId?: string
      model?: string
    }
  ): Promise<Conversation> {
    if (!isTauriRuntime()) return mockChatApi.updateConversation(conversationId, updates)
    const result = await invoke<{ success: boolean; conversation: Conversation }>(
      'chat_update_conversation',
      {
        conversationId,
        title: updates.title,
        pinned: updates.pinned,
        folder: updates.folder,
        providerId: updates.providerId,
        model: updates.model,
      }
    )
    if (!result.success) {
      throw new Error('Failed to update conversation')
    }
    return result.conversation
  },
}
