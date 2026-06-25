export interface ConversationStreamSnapshot {
  runId: string | null
  streaming: boolean
  content: string
  reasoning: string
  reasoningStreaming: boolean
  toolCalls: import('./types').ToolCallRecord[]
  segments: import('./types').ChatMessageSegment[]
  startedAt: number | null
  reasoningStartedAt: number | null
  reasoningDurationMs: number | null
  reasoningStartedAtBySegmentId: Record<string, number>
  reasoningDurationMsBySegmentId: Record<string, number>
}

export function isConversationInFlight(
  inFlightConversations: ReadonlySet<string>,
  conversationId: string,
): boolean {
  return inFlightConversations.has(conversationId)
}

export function isConversationBusy(
  conversationId: string | null | undefined,
  inFlightConversations: ReadonlySet<string>,
  streamSnapshots: Record<string, ConversationStreamSnapshot>,
): boolean {
  if (!conversationId) return false
  if (inFlightConversations.has(conversationId)) return true
  return streamSnapshots[conversationId]?.streaming === true
}

export function collectGeneratingConversationIds(
  inFlightConversations: ReadonlySet<string>,
  streamSnapshots: Record<string, ConversationStreamSnapshot>,
  pendingToolConfirms: Record<string, unknown>,
): Set<string> {
  const ids = new Set<string>(inFlightConversations)
  for (const [conversationId, snapshot] of Object.entries(streamSnapshots)) {
    if (snapshot.streaming) ids.add(conversationId)
  }
  for (const conversationId of Object.keys(pendingToolConfirms)) {
    ids.add(conversationId)
  }
  return ids
}

export function createEmptyStreamSnapshot(): ConversationStreamSnapshot {
  return {
    runId: null,
    streaming: true,
    content: '',
    reasoning: '',
    reasoningStreaming: false,
    toolCalls: [],
    segments: [],
    startedAt: Date.now(),
    reasoningStartedAt: null,
    reasoningDurationMs: null,
    reasoningStartedAtBySegmentId: {},
    reasoningDurationMsBySegmentId: {},
  }
}

/** Terminal status carried on a background sub-agent `chat-subagent` event. */
export type SubagentTerminalStatus = 'completed' | 'failed' | 'cancelled'

/**
 * Map a background sub-agent's terminal event status onto the parent tool
 * call's {@link import('./types').ToolCallStatus}. A detached (`background:true`)
 * spawn dispatches `is_error:false` immediately, pinning the tool call to
 * `completed`; without this remap a failed/cancelled sub-agent would keep
 * rendering as a green "completed" card (the card derives its visible status
 * from `normalizeToolCallStatus(toolCall.status)`, not from the progress view).
 * Returns `null` for a non-terminal status so callers leave the status as-is.
 */
export function terminalSubagentToolCallStatus(
  status: string | null | undefined,
): import('./types').ToolCallStatus | null {
  switch (status) {
    case 'failed':
      return 'error'
    case 'cancelled':
      return 'cancelled'
    case 'completed':
      return 'completed'
    default:
      return null
  }
}
