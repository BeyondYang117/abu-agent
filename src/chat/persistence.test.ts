/**
 * @vitest-environment jsdom
 */
import { describe, expect, it } from 'vitest'
import { hashPath, isChatPath } from './persistence'

describe('hashPath', () => {
  it('strips hash prefix and query string', () => {
    window.location.hash = '#chat/settings?tab=general'
    expect(hashPath()).toBe('chat/settings')
  })
})

describe('isChatPath', () => {
  it('matches chat routes', () => {
    expect(isChatPath('chat')).toBe(true)
    expect(isChatPath('chat/conv-1')).toBe(true)
    expect(isChatPath('settings')).toBe(false)
  })
})
