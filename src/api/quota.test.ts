import { describe, expect, it } from 'vitest'
import { ABU_QUOTA_PER_USD, formatAbuQuota, formatAbuQuotaReward } from './quota'

describe('ABU quota formatting', () => {
  it('converts API quota units using the server quota-per-unit', () => {
    expect(ABU_QUOTA_PER_USD).toBe(500000)
    expect(formatAbuQuota(998334469)).toBe('1996.67')
  })

  it('keeps small check-in rewards visible', () => {
    expect(formatAbuQuotaReward(1_000)).toBe('0.002')
    expect(formatAbuQuotaReward(5_000)).toBe('0.01')
  })
})
