import { describe, expect, it } from 'vitest'
import {
  actionForAgentErrorCode,
  describeOrganizationOption,
  parseAgentErrorCode,
} from './cloudContext'

describe('describeOrganizationOption', () => {
  it('keeps personal orgs selectable', () => {
    const option = describeOrganizationOption(
      {
        name: 'Personal',
        type: 'personal',
        status: 'active',
        activated: true,
        permissions: ['model.use'],
      },
      'zh',
    )
    expect(option.disabled).toBe(false)
    expect(option.label).toBe('Personal')
  })

  it('marks unactivated shared orgs but keeps them selectable', () => {
    const option = describeOrganizationOption(
      {
        name: 'Team',
        type: 'team',
        status: 'active',
        activated: false,
        permissions: ['model.use', 'wallet.view'],
      },
      'zh',
    )
    expect(option.disabled).toBe(false)
    expect(option.label).toContain('未激活')
    expect(option.reason).toContain('尚未激活')
  })

  it('disables orgs without model.use', () => {
    const option = describeOrganizationOption(
      {
        name: 'Finance',
        type: 'team',
        status: 'active',
        activated: true,
        permissions: ['wallet.view'],
      },
      'zh',
    )
    expect(option.disabled).toBe(true)
    expect(option.label).toContain('不可调用')
  })
})

describe('agent organization error routing', () => {
  it('parses bracketed error codes', () => {
    expect(parseAgentErrorCode('[ORG_NOT_ACTIVATED] 组织尚未激活')).toBe('ORG_NOT_ACTIVATED')
    expect(parseAgentErrorCode('plain message')).toBeNull()
  })

  it('maps activation errors to org top-up', () => {
    expect(actionForAgentErrorCode('ORG_NOT_ACTIVATED')).toBe('open_org_topup')
    expect(actionForAgentErrorCode('ORG_PRIVACY_CONSENT_REQUIRED')).toBe('open_privacy_consent')
  })
})
