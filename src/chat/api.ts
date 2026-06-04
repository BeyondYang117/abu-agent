// Chat API 调用封装
import { invoke } from '@tauri-apps/api/core'
import { estimateTokens } from '../lens/markdown'
import type { Conversation, ConversationContextState, ConversationListItem, PendingAttachment } from './types'

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

function estimateMockContext(conversation: Conversation): ConversationContextState {
  const conversationTokens = conversation.messages.reduce(
    (sum, message) => sum + estimateTokens(message.content || ''),
    0,
  )
  const attachmentTokens = conversation.messages.reduce(
    (sum, message) => sum + (message.attachments?.filter((attachment) => attachment.type === 'image').length ?? 0) * 1200,
    0,
  )
  const systemTokens = 900
  const estimatedInputTokens = systemTokens + conversationTokens + attachmentTokens
  const contextWindowTokens = 200_000
  const usageRatio = estimatedInputTokens / contextWindowTokens
  const summary = conversation.context_state?.summary ?? conversation.contextState?.summary ?? null
  return {
    estimated_input_tokens: estimatedInputTokens,
    context_window_tokens: contextWindowTokens,
    context_window_estimated: true,
    usage_ratio: usageRatio,
    status: summary?.stale
      ? 'stale'
      : summary
        ? 'compressed'
        : usageRatio >= 0.95
          ? 'critical'
          : usageRatio >= 0.70
            ? 'warning'
            : 'normal',
    segments: [
      { id: 'system_prompt', label: 'System prompt', estimated_tokens: systemTokens, color: '#7A7A7A' },
      { id: 'conversation', label: 'Conversation', estimated_tokens: conversationTokens, color: '#D07652' },
      { id: 'attachments', label: 'Attachments', estimated_tokens: attachmentTokens, color: '#6A8FBD' },
    ].filter((segment) => segment.estimated_tokens > 0),
    last_measured_at: nowSeconds(),
    last_compressed_at: summary?.created_at ?? summary?.createdAt ?? null,
    compressed_message_count: summary?.source_message_ids?.length ?? summary?.sourceMessageIds?.length ?? 0,
    summary,
  }
}

function withMockContext(conversation: Conversation): Conversation {
  const contextState = estimateMockContext(conversation)
  return {
    ...conversation,
    context_state: contextState,
    contextState,
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
    return withMockContext(conversation)
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
    const withContext = withMockContext(conversation)
    saveMockConversations([withContext, ...loadMockConversations()])
    return withContext
  },

  async sendMessage(
    conversationId: string,
    content: string,
    attachments: PendingAttachment[] = [],
    activeSkillId?: string | null,
  ): Promise<Conversation> {
    const conversations = loadMockConversations()
    const index = conversations.findIndex((item) => item.id === conversationId)
    if (index < 0) throw new Error('Conversation not found')
    const now = nowSeconds()
    const conversation = { ...conversations[index] }
    conversation.active_skill_id = activeSkillId ?? conversation.active_skill_id ?? conversation.activeSkillId ?? null
    conversation.activeSkillId = conversation.active_skill_id
    conversation.messages = [
      ...conversation.messages,
      {
        id: `msg_dev_${crypto.randomUUID()}`,
        role: 'user',
        content,
        attachments: attachments.map((attachment) => ({
          id: attachment.id,
          type: attachment.type,
          name: attachment.name,
          path: attachment.path,
        })),
        timestamp: now,
      },
      {
        id: `msg_dev_${crypto.randomUUID()}`,
        role: 'assistant',
        content: '这是浏览器预览模式的本地回复。启动 Tauri 桌面应用后会调用真实模型接口。',
        active_skill_id: conversation.active_skill_id,
        timestamp: now,
      },
    ]
    if (conversation.title === '新对话') {
      conversation.title = content.length > 30 ? `${content.slice(0, 30)}...` : content
    }
    conversation.updated_at = now
    const contextState = estimateMockContext(conversation)
    conversation.context_state = contextState
    conversation.contextState = contextState
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
      activeSkillId?: string | null
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
      active_skill_id:
        updates.activeSkillId !== undefined
          ? updates.activeSkillId || null
          : conversations[index].active_skill_id ?? conversations[index].activeSkillId ?? null,
      updated_at: nowSeconds(),
    }
    conversation.activeSkillId = conversation.active_skill_id
    const contextState = estimateMockContext(conversation)
    conversation.context_state = contextState
    conversation.contextState = contextState
    conversations[index] = conversation
    saveMockConversations(conversations)
    return conversation
  },

  async updateMessage(
    conversationId: string,
    messageId: string,
    content: string,
  ): Promise<Conversation> {
    const conversations = loadMockConversations()
    const index = conversations.findIndex((item) => item.id === conversationId)
    if (index < 0) throw new Error('Conversation not found')
    const trimmed = content.trim()
    if (!trimmed) throw new Error('消息内容不能为空')
    const conversation = { ...conversations[index] }
    const messageIndex = conversation.messages.findIndex((message) => message.id === messageId)
    if (messageIndex < 0) throw new Error('Message not found')
    if (conversation.messages[messageIndex].role !== 'assistant') {
      throw new Error('仅支持编辑助手回复')
    }
    conversation.messages = conversation.messages.map((message, i) =>
      i === messageIndex
        ? { ...message, content: trimmed, timestamp: nowSeconds() }
        : message,
    )
    conversation.updated_at = nowSeconds()
    const contextState = estimateMockContext(conversation)
    conversation.context_state = contextState
    conversation.contextState = contextState
    conversations[index] = conversation
    saveMockConversations(conversations)
    return conversation
  },

  async deleteMessage(conversationId: string, messageId: string): Promise<Conversation> {
    const conversations = loadMockConversations()
    const index = conversations.findIndex((item) => item.id === conversationId)
    if (index < 0) throw new Error('Conversation not found')
    const conversation = { ...conversations[index] }
    const target = conversation.messages.find((message) => message.id === messageId)
    if (!target) throw new Error('Message not found')
    if (target.role !== 'assistant') throw new Error('仅支持删除助手回复')
    conversation.messages = conversation.messages.filter((message) => message.id !== messageId)
    conversation.updated_at = nowSeconds()
    const contextState = estimateMockContext(conversation)
    conversation.context_state = contextState
    conversation.contextState = contextState
    conversations[index] = conversation
    saveMockConversations(conversations)
    return conversation
  },

  async regenerateMessage(conversationId: string, messageId: string): Promise<Conversation> {
    const conversations = loadMockConversations()
    const index = conversations.findIndex((item) => item.id === conversationId)
    if (index < 0) throw new Error('Conversation not found')
    const conversation = { ...conversations[index] }
    const messageIndex = conversation.messages.findIndex((message) => message.id === messageId)
    if (messageIndex < 0) throw new Error('Message not found')
    if (conversation.messages[messageIndex].role !== 'assistant') {
      throw new Error('仅支持重新生成助手回复')
    }
    const kept = conversation.messages.slice(0, messageIndex)
    const lastUser = kept[kept.length - 1]
    if (!lastUser || lastUser.role !== 'user') {
      throw new Error('缺少对应的用户消息，无法重新生成')
    }
    conversation.messages = [
      ...kept,
      {
        id: `msg_dev_${crypto.randomUUID()}`,
        role: 'assistant',
        content: `（重新生成预览）${lastUser.content.slice(0, 80)}`,
        timestamp: nowSeconds(),
      },
    ]
    conversation.updated_at = nowSeconds()
    const contextState = estimateMockContext(conversation)
    conversation.context_state = contextState
    conversation.contextState = contextState
    conversations[index] = conversation
    saveMockConversations(conversations)
    return conversation
  },

  async getContextStats(conversationId: string): Promise<{ contextState: ConversationContextState; conversation: Conversation }> {
    const conversations = loadMockConversations()
    const index = conversations.findIndex((item) => item.id === conversationId)
    if (index < 0) throw new Error('Conversation not found')
    const conversation = withMockContext(conversations[index])
    conversations[index] = conversation
    saveMockConversations(conversations)
    return { contextState: conversation.context_state ?? {}, conversation }
  },

  async compressContext(conversationId: string): Promise<{ contextState: ConversationContextState; conversation: Conversation }> {
    const conversations = loadMockConversations()
    const index = conversations.findIndex((item) => item.id === conversationId)
    if (index < 0) throw new Error('Conversation not found')
    const conversation = { ...conversations[index] }
    const cutoff = Math.max(0, conversation.messages.length - 8)
    const source = conversation.messages.slice(0, cutoff)
    if (source.length < 2) {
      throw new Error('没有足够的旧消息可以压缩')
    }
    const summary = {
      id: `ctxsum_dev_${crypto.randomUUID()}`,
      content: `Browser preview summary for ${source.length} older messages.`,
      source_message_ids: source.map((message) => message.id),
      source_until_message_id: source[source.length - 1]?.id ?? '',
      token_estimate_before: source.reduce((sum, message) => sum + estimateTokens(message.content || ''), 0),
      token_estimate_after: 20,
      created_at: nowSeconds(),
      provider_id: conversation.provider_id,
      model: conversation.model,
      stale: false,
    }
    const baseState = estimateMockContext(conversation)
    conversation.context_state = {
      ...baseState,
      status: 'compressed',
      summary,
      last_compressed_at: summary.created_at,
      compressed_message_count: source.length,
      segments: [
        ...(baseState.segments ?? []).filter((segment) => segment.id !== 'summarized_conversation'),
        { id: 'summarized_conversation', label: 'Summarized conversation', estimated_tokens: 20, color: '#BF3F66' },
      ],
    }
    conversation.contextState = conversation.context_state
    conversations[index] = conversation
    saveMockConversations(conversations)
    return { contextState: conversation.context_state, conversation }
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
    attachments: PendingAttachment[] = [],
    activeSkillId?: string | null,
  ): Promise<Conversation> {
    if (!isTauriRuntime()) {
      return mockChatApi.sendMessage(conversationId, content, attachments, activeSkillId)
    }
    const result = await invoke<{ success: boolean; conversation?: Conversation; error?: string }>(
      'chat_send_message',
      {
        conversationId,
        content,
        attachments: attachments.map((attachment) => attachment.path),
        activeSkillId,
      }
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
      activeSkillId?: string | null
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
        activeSkillId: updates.activeSkillId,
      }
    )
    if (!result.success) {
      throw new Error('Failed to update conversation')
    }
    return result.conversation
  },

  async updateMessage(
    conversationId: string,
    messageId: string,
    content: string,
  ): Promise<Conversation> {
    if (!isTauriRuntime()) {
      return mockChatApi.updateMessage(conversationId, messageId, content)
    }
    const result = await invoke<{
      success: boolean
      conversation?: Conversation
      error?: string
    }>('chat_update_message', { conversationId, messageId, content })
    if (!result.success || !result.conversation) {
      throw new Error(result.error || 'Failed to update message')
    }
    return result.conversation
  },

  async deleteMessage(conversationId: string, messageId: string): Promise<Conversation> {
    if (!isTauriRuntime()) {
      return mockChatApi.deleteMessage(conversationId, messageId)
    }
    const result = await invoke<{
      success: boolean
      conversation?: Conversation
      error?: string
    }>('chat_delete_message', { conversationId, messageId })
    if (!result.success || !result.conversation) {
      throw new Error(result.error || 'Failed to delete message')
    }
    return result.conversation
  },

  async regenerateMessage(conversationId: string, messageId: string): Promise<Conversation> {
    if (!isTauriRuntime()) {
      return mockChatApi.regenerateMessage(conversationId, messageId)
    }
    const result = await invoke<{
      success: boolean
      conversation?: Conversation
      error?: string
    }>('chat_regenerate_message', { conversationId, messageId })
    if (!result.success || !result.conversation) {
      throw new Error(result.error || 'Failed to regenerate message')
    }
    return result.conversation
  },

  async getContextStats(conversationId: string): Promise<{ contextState: ConversationContextState; conversation: Conversation }> {
    if (!isTauriRuntime()) return mockChatApi.getContextStats(conversationId)
    const result = await invoke<{
      success: boolean
      contextState?: ConversationContextState
      conversation?: Conversation
      error?: string
    }>('chat_get_context_stats', { conversationId })
    if (!result.success || !result.contextState || !result.conversation) {
      throw new Error(result.error || 'Failed to get context stats')
    }
    return { contextState: result.contextState, conversation: result.conversation }
  },

  async compressContext(conversationId: string): Promise<{ contextState: ConversationContextState; conversation: Conversation }> {
    if (!isTauriRuntime()) return mockChatApi.compressContext(conversationId)
    const result = await invoke<{
      success: boolean
      contextState?: ConversationContextState
      conversation?: Conversation
      error?: string
    }>('chat_compress_context', { conversationId })
    if (!result.success || !result.contextState || !result.conversation) {
      throw new Error(result.error || 'Failed to compress context')
    }
    return { contextState: result.contextState, conversation: result.conversation }
  },

  async cancelStream(conversationId: string): Promise<void> {
    if (!isTauriRuntime()) return
    await invoke<void>('chat_cancel_stream', { conversationId })
  },
}
