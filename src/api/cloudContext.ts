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

export async function setActiveOrganization(organizationId: string): Promise<CloudContextView | null> {
  const view = await api.cloudContextSetOrganization(organizationId)
  cloudContextStore.setView(view)
  return view
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
