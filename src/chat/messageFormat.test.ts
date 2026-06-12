import { describe, expect, it } from 'vitest'
import { formatAssistantMessageTime } from './messageFormat'

describe('formatAssistantMessageTime', () => {
  it('formats timestamps in en-US style', () => {
    const formatted = formatAssistantMessageTime(1767225600)
    expect(formatted).toMatch(/Jan 1, 2026 at \d{1,2}:\d{2} (AM|PM)/)
  })
})
