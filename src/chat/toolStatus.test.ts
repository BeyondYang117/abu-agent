import { describe, expect, it } from 'vitest'
import { isToolCallErrorStatus, normalizeToolCallStatus } from './toolStatus'

describe('normalizeToolCallStatus', () => {
  it('maps running aliases to running', () => {
    expect(normalizeToolCallStatus('running')).toBe('running')
    expect(normalizeToolCallStatus('in_progress')).toBe('running')
    expect(normalizeToolCallStatus('calling')).toBe('running')
    expect(normalizeToolCallStatus('executing')).toBe('running')
  })

  it('maps success aliases to completed', () => {
    expect(normalizeToolCallStatus('completed')).toBe('completed')
    expect(normalizeToolCallStatus('success')).toBe('completed')
    expect(normalizeToolCallStatus('succeeded')).toBe('completed')
  })

  it('maps error aliases to error', () => {
    expect(normalizeToolCallStatus('error')).toBe('error')
    expect(normalizeToolCallStatus('failed')).toBe('error')
  })

  it('maps cancelled aliases to cancelled', () => {
    expect(normalizeToolCallStatus('cancelled')).toBe('cancelled')
    expect(normalizeToolCallStatus('canceled')).toBe('cancelled')
  })

  it('defaults unknown statuses to pending', () => {
    expect(normalizeToolCallStatus(undefined)).toBe('pending')
    expect(normalizeToolCallStatus('queued')).toBe('pending')
    expect(normalizeToolCallStatus('unknown')).toBe('pending')
  })
})

describe('isToolCallErrorStatus', () => {
  it('returns true only for error statuses', () => {
    expect(isToolCallErrorStatus('error')).toBe(true)
    expect(isToolCallErrorStatus('failed')).toBe(true)
  })

  it('does not treat success as error (regression: tool UI red preview)', () => {
    expect(isToolCallErrorStatus('success')).toBe(false)
    expect(isToolCallErrorStatus('completed')).toBe(false)
    expect(isToolCallErrorStatus('running')).toBe(false)
  })
})
