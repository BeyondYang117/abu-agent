import { describe, expect, it } from 'vitest'
import { ABU_API_BASE_URLS, ABU_PLATFORM_URL, getAbuApiEndpointCandidates, normalizeAbuApiBaseUrl } from './abuApiEndpoints'

describe('ABU platform endpoints', () => {
  it('uses the primary host for platform pages', () => {
    expect(ABU_PLATFORM_URL).toBe('https://api.abuai.chat')
  })

  it('does not offer the API-only domain for login authorization', () => {
    expect(ABU_API_BASE_URLS).not.toContain('https://api.abu117.cn')
    expect(getAbuApiEndpointCandidates()).not.toContain('https://api.abu117.cn')
  })

  it('migrates a previously saved API-only domain to the default platform', () => {
    expect(normalizeAbuApiBaseUrl('https://api.abu117.cn/')).toBe('https://api.abuai.chat')
    expect(getAbuApiEndpointCandidates('https://api.abu117.cn')).toEqual([
      'https://api.abuai.chat',
      'https://api.abusz.com',
    ])
  })
})
