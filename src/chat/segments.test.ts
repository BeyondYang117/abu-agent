import { describe, expect, it } from 'vitest'
import type { ChatMessageSegment } from './types'
import { compareTimelineSegments, segmentToolCallId } from './segments'

function segment(partial: Partial<ChatMessageSegment> & Pick<ChatMessageSegment, 'id' | 'kind' | 'order'>): ChatMessageSegment {
  return {
    phase: 'plain',
    ...partial,
  }
}

describe('segmentToolCallId', () => {
  it('prefers snake_case tool_call_id', () => {
    expect(segmentToolCallId({ tool_call_id: 'a', toolCallId: 'b' } as ChatMessageSegment)).toBe('a')
  })

  it('falls back to camelCase toolCallId', () => {
    expect(segmentToolCallId({ toolCallId: 'b' } as ChatMessageSegment)).toBe('b')
  })
})

describe('compareTimelineSegments', () => {
  it('orders reasoning before text within the same model step', () => {
    const reasoning = segment({
      id: 'r',
      kind: 'reasoning',
      order: 2,
      step_number: 1,
      round: 0,
      phase: 'tool_loop',
    })
    const text = segment({
      id: 't',
      kind: 'text',
      order: 1,
      step_number: 1,
      round: 0,
      phase: 'tool_loop',
    })
    expect(compareTimelineSegments(reasoning, text)).toBeLessThan(0)
    expect(compareTimelineSegments(text, reasoning)).toBeGreaterThan(0)
  })

  it('falls back to order when model steps differ', () => {
    const earlier = segment({ id: 'a', kind: 'text', order: 1, step_number: 1 })
    const later = segment({ id: 'b', kind: 'reasoning', order: 2, step_number: 2 })
    expect(compareTimelineSegments(earlier, later)).toBeLessThan(0)
  })
})
