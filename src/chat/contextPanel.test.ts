import { describe, expect, it } from 'vitest'
import { buildContextBarSlices, CONTEXT_FREE_SEGMENT_ID } from './contextPanel'
import { i18n } from '../settings/i18n'

describe('buildContextBarSlices', () => {
  const t = i18n.zh

  it('includes free space slice when window is known', () => {
    const slices = buildContextBarSlices(
      [
        { id: 'conversation', label: 'Conversation', estimated_tokens: 50_000 },
        { id: 'attachments', label: 'Attachments', estimated_tokens: 10_000 },
      ],
      60_000,
      200_000,
      t,
    )
    const free = slices.find((slice) => slice.id === CONTEXT_FREE_SEGMENT_ID)
    expect(free?.tokens).toBe(140_000)
    expect(free?.widthPercent).toBeCloseTo(70, 1)
    expect(slices.reduce((sum, slice) => sum + slice.widthPercent, 0)).toBeCloseTo(100, 1)
  })
})
