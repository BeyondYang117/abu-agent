import { describe, expect, it } from 'vitest'
import { shouldOpenSlashPopover } from './slashCommands'

describe('shouldOpenSlashPopover', () => {
  it('opens whenever a slash token is active', () => {
    expect(shouldOpenSlashPopover()).toBe(true)
  })
})
