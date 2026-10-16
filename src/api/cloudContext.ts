/**
 * Cloud organization context (WS-13 / C §3).
 * React only sees desensitized views from Rust CloudContextManager.
 */

import { useEffect, useState } from 'react'
import { api, type CloudContextView } from './tauri'

type Listener = () => void

class CloudContextStore {
  private view: CloudContextView | null = null
  private listeners = new Set<Listener>()

  getView(): CloudContextView | null {
    return this.view
  }

  setView(view: CloudContextView | null) {
    this.view = view
    this.emit()
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  private emit() {
    this.listeners.forEach((listener) => listener())
  }
}

export const cloudContextStore = new CloudContextStore()

export function useCloudContext(): CloudContextView | null {
  const [view, setView] = useState(cloudContextStore.getView())
  useEffect(() => cloudContextStore.subscribe(() => setView(cloudContextStore.getView())), [])
  return view
}

export async function refreshCloudContext(): Promise<CloudContextView | null> {
  try {
    const view = await api.cloudContextRefresh()
    cloudContextStore.setView(view)
    if (view.clientUpgradeRequired) {
      void handleClientUpgradeRequired(view.clientUpgradeDownloadUrl)
    }
    return view
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    if (message.includes('CLIENT_UPGRADE_REQUIRED')) {
      void handleClientUpgradeRequired(null)
    }
    console.warn('Failed to refresh cloud context:', err)
    return null
  }
}

export async function setActiveOrganization(
  organizationId: string,
  options?: { privacyConsentAccepted?: boolean },
): Promise<CloudContextView | null> {
  const view = await api.cloudContextSetOrganization(
    organizationId,
    options?.privacyConsentAccepted,
  )
  cloudContextStore.setView(view)
  return view
}

/** Open platform org management / top-up for the given organization. */
export async function openOrganizationBillingPage(organizationId?: string | null) {
  const { ABU_PLATFORM_URL } = await import('./abuApiEndpoints')
  const id = String(organizationId || '').trim()
  const path = id
    ? `/console/organizations?organization_id=${encodeURIComponent(id)}`
    : '/console/organizations'
  await api.openExternal(`${ABU_PLATFORM_URL}${path}`)
}

export function describeOrganizationOption(
  org: {
    name: string
    type: string
    status: string
    activated?: boolean
    privacy_consent_required?: boolean
    permissions?: string[]
  },
  lang: 'zh' | 'en',
): { label: string; disabled: boolean; reason?: string } {
  const canUseModel = (org.permissions || []).includes('model.use')
  const active = org.status === 'active'
  const activated = org.type === 'personal' || org.activated !== false
  const tags: string[] = []
  if (org.type !== 'personal') tags.push(org.type)
  if (!active) tags.push(lang === 'zh' ? `状态:${org.status}` : org.status)
  else if (!activated) tags.push(lang === 'zh' ? '未激活' : 'inactive')
  if (!canUseModel) tags.push(lang === 'zh' ? '不可调用' : 'no model.use')
  if (org.privacy_consent_required) tags.push(lang === 'zh' ? '需同意隐私' : 'consent needed')
  const label = tags.length > 0 ? `${org.name} · ${tags.join(' · ')}` : org.name
  // 决策理由：无 model.use 或非 active 禁止切换；未激活仍可选以便 Owner 去充值。
  const disabled = !active || !canUseModel
  let reason: string | undefined
  if (!active) reason = lang === 'zh' ? '组织状态不可用' : 'Organization not active'
  else if (!canUseModel) reason = lang === 'zh' ? '当前角色不能调用模型' : 'Role cannot use models'
  else if (!activated) reason = lang === 'zh' ? '组织尚未激活，调用前需先充值' : 'Activate org wallet first'
  return { label, disabled, reason }
}

export function handleAgentOrganizationError(
  err: unknown,
  lang: 'zh' | 'en',
  organizationId?: string | null,
): { code: string | null; message: string; action: string } {
  const message = err instanceof Error ? err.message : String(err || '')
  const code = parseAgentErrorCode(message)
  const action = code ? actionForAgentErrorCode(code) : 'none'
  return { code, message, action }
}

export async function runAgentOrganizationErrorAction(
  action: string,
  organizationId?: string | null,
): Promise<void> {
  switch (action) {
    case 'open_org_topup':
    case 'open_billing':
    case 'open_privacy_consent':
    case 'switch_organization':
      await openOrganizationBillingPage(organizationId)
      break
    case 'open_update':
      try {
        await api.checkUpdate()
      } catch {
        // ignore
      }
      break
    default:
      break
  }
}

export function organizationsFeatureEnabled(view: CloudContextView | null): boolean {
  if (!view || view.legacyMode) return false
  return Boolean(view.bootstrap?.features?.organizations)
}

async function handleClientUpgradeRequired(downloadUrl: string | null | undefined) {
  try {
    // C §8.4 / §11.3 — open built-in update flow.
    await api.checkUpdate()
    return
  } catch (err) {
    console.warn('Built-in update check failed:', err)
  }
  if (downloadUrl) {
    try {
      await api.openExternal(downloadUrl)
    } catch (err) {
      console.warn('Failed to open upgrade URL:', err)
    }
  }
}

/** Map stable error codes to user-facing action hints (C §8.4). */
export function actionForAgentErrorCode(code: string): string {
  switch (code.toUpperCase()) {
    case 'CLIENT_UPGRADE_REQUIRED':
      return 'open_update'
    case 'ORG_PRIVACY_CONSENT_REQUIRED':
      return 'open_privacy_consent'
    case 'ORG_CLOSING':
    case 'ORG_MEMBERSHIP_REVOKED':
      return 'switch_organization'
    case 'BUDGET_EXHAUSTED':
    case 'ORG_BALANCE_INSUFFICIENT':
      return 'open_billing'
    case 'ORG_NOT_ACTIVATED':
      return 'open_org_topup'
    case 'PRICE_VERSION_EXPIRED':
      return 'refresh_task'
    case 'ORG_BINDING_REQUIRED':
      return 'new_conversation'
    default:
      return 'none'
  }
}

export function parseAgentErrorCode(message: string): string | null {
  const match = message.match(/^\[([A-Z0-9_]+)\]/)
  return match?.[1] ?? null
}
