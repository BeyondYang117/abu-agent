import type { ChatMessage, CompactionBoundaryRecord, ConversationContextState } from './types'

export interface CompactionBoundaryView {
  afterIndex: number
  record: CompactionBoundaryRecord
}

function readUntilId(record: CompactionBoundaryRecord): string | null {
  return record.source_until_message_id ?? record.sourceUntilMessageId ?? null
}

function readSummaryContent(record: CompactionBoundaryRecord): string {
  return record.summary_content ?? record.summaryContent ?? ''
}

function readTokenBefore(record: CompactionBoundaryRecord): number {
  return record.token_estimate_before ?? record.tokenEstimateBefore ?? 0
}

function readTokenAfter(record: CompactionBoundaryRecord): number {
  return record.token_estimate_after ?? record.tokenEstimateAfter ?? 0
}

function legacySummaryToRecord(
  summary: NonNullable<ConversationContextState['summary']>,
): CompactionBoundaryRecord | null {
  const sourceUntil = summary.source_until_message_id ?? summary.sourceUntilMessageId
  if (!sourceUntil || summary.stale) return null
  return {
    id: summary.id,
    source_until_message_id: sourceUntil,
    token_estimate_before: summary.token_estimate_before ?? summary.tokenEstimateBefore ?? 0,
    token_estimate_after: summary.token_estimate_after ?? summary.tokenEstimateAfter ?? 0,
    summary_content: summary.content,
    trigger: 'auto',
    created_at: summary.created_at ?? summary.createdAt,
  }
}

export function collectCompactionRecords(
  contextState?: ConversationContextState | null,
): CompactionBoundaryRecord[] {
  if (!contextState) return []
  const explicit = contextState.compaction_boundaries ?? contextState.compactionBoundaries ?? []
  if (explicit.length > 0) return explicit
  const summary = contextState.summary
  if (!summary) return []
  const legacy = legacySummaryToRecord(summary)
  return legacy ? [legacy] : []
}

export function resolveCompactionBoundaries(
  messages: ChatMessage[],
  contextState?: ConversationContextState | null,
): CompactionBoundaryView[] {
  const records = collectCompactionRecords(contextState)
  const views: CompactionBoundaryView[] = []

  for (const record of records) {
    const untilId = readUntilId(record)
    if (!untilId) continue
    const afterIndex = messages.findIndex((message) => message.id === untilId)
    if (afterIndex < 0) continue
    views.push({ afterIndex, record })
  }

  views.sort((a, b) => a.afterIndex - b.afterIndex || (a.record.created_at ?? 0) - (b.record.created_at ?? 0))
  return views
}

export function hasCompactionTokenDetail(record: CompactionBoundaryRecord): boolean {
  return readTokenBefore(record) > 0 || readTokenAfter(record) > 0
}

export function compactionTriggerLabel(trigger: string | undefined, t: { contextCompactionTriggerManual: string; contextCompactionTriggerAuto: string; contextCompactionTriggerAgentLoop: string; contextCompactionTriggerGeneric: string }): string {
  if (trigger === 'manual') return t.contextCompactionTriggerManual
  if (trigger === 'auto') return t.contextCompactionTriggerAuto
  if (trigger === 'agent_loop') return t.contextCompactionTriggerAgentLoop
  return t.contextCompactionTriggerGeneric
}

export function compactionRecordTokens(record: CompactionBoundaryRecord) {
  return {
    before: readTokenBefore(record),
    after: readTokenAfter(record),
    summary: readSummaryContent(record),
  }
}

/** Aligns with backend `summary_boundary_index` + 1 when an active summary exists. */
function summaryBoundaryIndex(
  messages: ChatMessage[],
  contextState?: ConversationContextState | null,
): number | null {
  const summary = contextState?.summary
  if (!summary || summary.stale) return null
  const untilId = summary.source_until_message_id ?? summary.sourceUntilMessageId
  if (!untilId) return null
  const index = messages.findIndex((message) => message.id === untilId)
  return index >= 0 ? index : null
}

/** 与后端 `compaction::RECENT_KEEP_TOKENS` 对齐的近期尾窗预算（tokens）。 */
const RECENT_KEEP_TOKENS = 20_000

/** 粗略 token 估算（与后端 `estimate_tokens` 的 ASCII≈4 chars/token 启发式一致）。 */
function approxTokenCount(text: string | undefined | null): number {
  if (!text) return 0
  return Math.ceil(text.length / 4)
}

/** 单条 UI 消息的 token 估算：content + reasoning + 工具入参 + 结果预览（对齐后端
 * `estimate_chat_message_tokens`）。 */
function estimateChatMessageTokens(message: ChatMessage): number {
  let total = approxTokenCount(message.content)
  if (message.reasoning) total += approxTokenCount(message.reasoning)
  for (const tool of message.tool_calls ?? []) {
    total += approxTokenCount(typeof tool.name === 'string' ? tool.name : '')
    total += approxTokenCount(
      typeof tool.arguments === 'string' ? tool.arguments : JSON.stringify(tool.arguments ?? ''),
    )
    total += approxTokenCount(tool.result_preview ?? undefined)
    total += approxTokenCount(tool.error ?? undefined)
  }
  return total + 1
}

/**
 * Best-effort 预估下一次手动/自动压缩 divider 落点（与后端 `token_split_chat_messages` 对齐）：
 * 在上一份未过期 summary 之后，从尾部往前累积整条消息的估算 token，直到 ~`RECENT_KEEP_TOKENS`
 * 预算用尽；越预算的那条整体归入旧段，其前一条即为 divider 落点（old_segment 末尾）。
 * 全部消息都在近期尾窗内（无旧段）→ 返回 null。
 */
export function estimatePendingCompactionAfterIndex(
  messages: ChatMessage[],
  contextState?: ConversationContextState | null,
): number | null {
  if (messages.length < 2) return null
  const minBoundary = (summaryBoundaryIndex(messages, contextState) ?? -1) + 1
  const maxBoundary = messages.length - 1
  if (minBoundary > maxBoundary) return null
  let total = 0
  let split = messages.length
  for (let index = maxBoundary; index >= minBoundary; index -= 1) {
    const next = total + estimateChatMessageTokens(messages[index])
    if (next > RECENT_KEEP_TOKENS && index + 1 < messages.length) {
      split = index + 1
      break
    }
    total = next
    split = index
  }
  if (split <= minBoundary) return null
  return split - 1
}

/** Resolve the timeline slot for an in-flight compaction (estimate → actual boundary). */
export function resolvePendingCompactionAfterIndex(
  messages: ChatMessage[],
  contextState: ConversationContextState | null | undefined,
  pendingBoundaryId?: string | null,
): number | null {
  if (pendingBoundaryId) {
    const match = resolveCompactionBoundaries(messages, contextState)
      .find((view) => view.record.id === pendingBoundaryId)
    if (match) return match.afterIndex
  }
  const estimated = estimatePendingCompactionAfterIndex(messages, contextState)
  if (estimated !== null) return estimated
  // Token tail window covers the whole conversation (small context windows, or a
  // short-but-triggered compaction) → the token estimate has no old segment. Fall back
  // to the last assistant message after the summary boundary so the in-progress
  // compaction animation still has a slot to render at (matches pre-token-window behavior).
  const minBoundary = (summaryBoundaryIndex(messages, contextState) ?? -1) + 1
  for (let index = messages.length - 1; index >= minBoundary; index -= 1) {
    if (messages[index]?.role === 'assistant') return index
  }
  return null
}

export function latestCompactionBoundaryId(
  contextState?: ConversationContextState | null,
): string | null {
  const records = collectCompactionRecords(contextState)
  const latest = records[records.length - 1]
  return latest?.id ?? null
}

/** Preserve compaction timeline markers when a partial context refresh omits them. */
export function mergeCompactionContextState(
  prev: ConversationContextState | null | undefined,
  next: ConversationContextState,
): ConversationContextState {
  const prevRecords = collectCompactionRecords(prev)
  const nextRecords = collectCompactionRecords(next)
  if (nextRecords.length >= prevRecords.length) return next
  const byId = new Map<string, CompactionBoundaryRecord>()
  for (const record of prevRecords) byId.set(record.id, record)
  for (const record of nextRecords) byId.set(record.id, record)
  const merged = [...byId.values()].sort(
    (a, b) => (a.created_at ?? a.createdAt ?? 0) - (b.created_at ?? b.createdAt ?? 0),
  )
  return {
    ...next,
    compaction_boundaries: merged,
    compactionBoundaries: merged,
  }
}
