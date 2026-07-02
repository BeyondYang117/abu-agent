import { describe, expect, it } from 'vitest'
import { collectCompactionRecords, estimatePendingCompactionAfterIndex, resolveCompactionBoundaries, resolvePendingCompactionAfterIndex } from './compactionBoundary'
import type { ChatMessage, ConversationContextState } from './types'

const messages: ChatMessage[] = [
  { id: 'm1', role: 'user', content: 'hello', timestamp: 1 },
  { id: 'm2', role: 'assistant', content: 'hi', timestamp: 2 },
  { id: 'm3', role: 'user', content: 'more', timestamp: 3 },
]

describe('compactionBoundary', () => {
  it('resolves explicit compaction boundaries by message id', () => {
    const contextState: ConversationContextState = {
      compaction_boundaries: [{
        id: 'b1',
        source_until_message_id: 'm2',
        token_estimate_before: 42000,
        token_estimate_after: 3200,
        summary_content: 'summary text',
        trigger: 'manual',
        created_at: 10,
      }],
    }
    const views = resolveCompactionBoundaries(messages, contextState)
    expect(views).toHaveLength(1)
    expect(views[0]?.afterIndex).toBe(1)
    expect(views[0]?.record.summary_content).toBe('summary text')
  })

  it('falls back to legacy summary when boundaries array is empty', () => {
    const contextState: ConversationContextState = {
      summary: {
        id: 's1',
        content: 'legacy summary',
        source_until_message_id: 'm1',
        token_estimate_before: 1000,
        token_estimate_after: 200,
        stale: false,
      },
    }
    const records = collectCompactionRecords(contextState)
    expect(records).toHaveLength(1)
    expect(records[0]?.trigger).toBe('auto')
    expect(resolveCompactionBoundaries(messages, contextState)[0]?.afterIndex).toBe(0)
  })

    it('skips stale legacy summary', () => {
    const contextState: ConversationContextState = {
      summary: {
        id: 's1',
        content: 'stale',
        source_until_message_id: 'm1',
        stale: true,
      },
    }
    expect(collectCompactionRecords(contextState)).toHaveLength(0)
  })

  it('estimates pending compaction boundary index (token tail window)', () => {
    // 每条 ~7500 tokens（30000 chars / 4）；4 条总 30000 > 20000 尾窗 → 从尾累积 2 条
    // (15000) 后第 3 条越预算 → divider 落在 index 1（old_segment=[m0,m1], recent=[m2,m3]）。
    const longThread = Array.from({ length: 4 }, (_, index) => ({
      id: `m${index}`,
      role: (index % 2 === 0 ? 'user' : 'assistant') as 'user' | 'assistant',
      content: 'x'.repeat(30_000),
      timestamp: index,
    }))
    expect(estimatePendingCompactionAfterIndex(longThread)).toBe(1)
  })

  it('returns null when recent tail window covers all messages', () => {
    // 全是小消息，远不到 20k 尾窗 → 没有可压缩旧段。
    const smallThread = Array.from({ length: 12 }, (_, index) => ({
      id: `m${index}`,
      role: (index % 2 === 0 ? 'user' : 'assistant') as 'user' | 'assistant',
      content: `msg ${index}`,
      timestamp: index,
    }))
    expect(estimatePendingCompactionAfterIndex(smallThread)).toBeNull()
  })

  it('estimates re-compression after existing summary boundary (token tail window)', () => {
    // summary source_until = m3 → minBoundary = 4。m4/m5 各 ~12500 tokens（50000 chars），
    // 从尾累积：m5(12500) 进窗口，m4(12500) → 25000 > 20000 → divider 落在 index 4。
    const longThread = Array.from({ length: 6 }, (_, index) => ({
      id: `m${index}`,
      role: (index % 2 === 0 ? 'user' : 'assistant') as 'user' | 'assistant',
      content: 'x'.repeat(50_000),
      timestamp: index,
    }))
    const contextState: ConversationContextState = {
      summary: {
        id: 's1',
        content: 'old summary',
        source_until_message_id: 'm3',
        token_estimate_before: 1000,
        token_estimate_after: 200,
        stale: false,
      },
    }
    expect(estimatePendingCompactionAfterIndex(longThread, contextState)).toBe(4)
  })

  it('prefers actual boundary id over estimate while compacting', () => {
    const contextState: ConversationContextState = {
      compaction_boundaries: [{
        id: 'b-new',
        source_until_message_id: 'm2',
        token_estimate_before: 1000,
        token_estimate_after: 200,
        summary_content: 'summary',
        trigger: 'manual',
        created_at: 10,
      }],
    }
    expect(resolvePendingCompactionAfterIndex(messages, contextState, 'b-new')).toBe(1)
  })

  it('falls back to last assistant so the in-progress animation shows on small conversations', () => {
    // Tail window covers the whole (small) conversation → token estimate is null,
    // but a triggered compaction must still render its animation somewhere.
    expect(estimatePendingCompactionAfterIndex(messages)).toBeNull()
    expect(resolvePendingCompactionAfterIndex(messages, null)).toBe(1) // m2 (assistant)
  })
})
