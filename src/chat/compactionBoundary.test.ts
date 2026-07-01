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

  it('estimates pending compaction boundary index', () => {
    const longThread = Array.from({ length: 12 }, (_, index) => ({
      id: `m${index}`,
      role: (index % 2 === 0 ? 'user' : 'assistant') as 'user' | 'assistant',
      content: `msg ${index}`,
      timestamp: index,
    }))
    expect(estimatePendingCompactionAfterIndex(longThread)).toBe(11)
  })

  it('estimates re-compression after existing summary boundary', () => {
    const longThread = Array.from({ length: 20 }, (_, index) => ({
      id: `m${index}`,
      role: (index % 2 === 0 ? 'user' : 'assistant') as 'user' | 'assistant',
      content: `msg ${index}`,
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
    expect(estimatePendingCompactionAfterIndex(longThread, contextState)).toBe(19)
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
})
