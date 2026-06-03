// Chat 前端类型定义

export const TOOL_CALL_STATUSES = [
  'pending',
  'running',
  'success',
  'completed',
  'error',
  'skipped',
  'cancelled',
] as const

export type ToolCallStatus = (typeof TOOL_CALL_STATUSES)[number]

export interface SkillMeta {
  id: string
  name: string
  description: string
  source?: 'builtin' | 'user' | 'external' | string
  path?: string
  recommended_tools?: string[]
  recommendedTools?: string[]
  enabled?: boolean
}

export interface SkillDetail extends SkillMeta {
  content?: string
  body?: string
  frontmatter?: Record<string, unknown>
  updated_at?: number
  updatedAt?: number
}

export interface ToolCallRecord {
  id: string
  conversationId?: string
  runId?: string
  messageId?: string
  toolCallId?: string
  call_id?: string
  callId?: string
  tool_name?: string
  toolName?: string
  name?: string
  server_id?: string
  serverId?: string
  server_name?: string
  serverName?: string
  source?: string
  status?: ToolCallStatus
  started_at?: number
  startedAt?: number
  completed_at?: number
  completedAt?: number
  duration_ms?: number
  durationMs?: number
  arguments?: unknown
  args?: unknown
  input?: unknown
  argument_preview?: string
  argumentPreview?: string
  argumentsPreview?: string
  result?: unknown
  output?: unknown
  result_preview?: string
  resultPreview?: string
  error?: string
  round?: number
  sensitive?: boolean
  requires_confirmation?: boolean
  requiresConfirmation?: boolean
}

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  attachments?: Attachment[]
  reasoning?: string
  tool_calls?: ToolCallRecord[]
  toolCalls?: ToolCallRecord[]
  active_skill_id?: string | null
  activeSkillId?: string | null
  timestamp: number
}

export interface Attachment {
  id: string
  type: 'image' | 'file'
  name: string
  path: string
}

export interface PendingAttachment {
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
  active_skill_id?: string | null
  activeSkillId?: string | null
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
