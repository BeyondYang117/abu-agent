import { describe, expect, it } from 'vitest'
import { resolveAccountDisplayName } from './accountDisplayName'

describe('account display name', () => {
  it('prefers the username without an @ prefix', () => {
    expect(resolveAccountDisplayName({ username: '@abu117', displayName: 'Root User' }, 'ABU Agent')).toBe('abu117')
  })

  it('falls back to display name when username is empty', () => {
    expect(resolveAccountDisplayName({ username: '', displayName: 'Root User' }, 'ABU Agent')).toBe('Root User')
  })
})
