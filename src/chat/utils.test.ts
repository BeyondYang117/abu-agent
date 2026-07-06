import { describe, expect, it, vi } from 'vitest'
import {
  CHAT_EMPTY_GREETINGS,
  pickRandomChatEmptyGreeting,
} from './utils'

describe('pickRandomChatEmptyGreeting', () => {
  it('returns one of the configured greetings', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0)
    expect(pickRandomChatEmptyGreeting()).toBe(CHAT_EMPTY_GREETINGS[0])
    vi.restoreAllMocks()
  })
})
