export const CHAT_DEFAULT_SIZE = { width: 1280, height: 800 }
/** 侧栏收起时可缩到的最小尺寸 */
export const CHAT_MIN_SIZE_COLLAPSED = { width: 400, height: 400 }
/** 侧栏展开时整窗最小尺寸（260px 侧栏 + 主内容区） */
export const CHAT_MIN_SIZE_EXPANDED = { width: 660, height: 400 }
export const CHAT_MIN_SIZE = CHAT_MIN_SIZE_COLLAPSED

const CHAT_LAST_ROUTE_KEY = 'kivio-chat-last-route'
const CHAT_WINDOW_SIZE_KEY = 'kivio-chat-window-size'

export function hashPath(): string {
  return window.location.hash.replace('#', '').split('?')[0]
}

export function isChatPath(path: string): boolean {
  return path === 'chat' || path.startsWith('chat/')
}

export function isChatSettingsPath(path: string): boolean {
  return path === 'chat/settings' || path.startsWith('chat/settings/')
}

function getLocalStorageItem(key: string): string | null {
  try {
    return window.localStorage?.getItem(key) ?? null
  } catch {
    return null
  }
}

function setLocalStorageItem(key: string, value: string) {
  try {
    window.localStorage?.setItem(key, value)
  } catch {
    // Storage can be unavailable in restricted previews. Chat still works without persistence.
  }
}

function removeLocalStorageItem(key: string) {
  try {
    window.localStorage?.removeItem(key)
  } catch {
    // Ignore storage errors; persistence is best-effort only.
  }
}

export function normalizeStoredChatRoute(value: string | null): string | null {
  if (!value) return null
  const route = value.startsWith('#') ? value : `#${value}`
  const path = route.replace('#', '').split('?')[0]
  if (!isChatPath(path) || isChatSettingsPath(path)) return null
  return route
}

export function rememberCurrentChatRoute() {
  const path = hashPath()
  if (!path.startsWith('chat/') || isChatSettingsPath(path)) return
  setLocalStorageItem(CHAT_LAST_ROUTE_KEY, window.location.hash || '#chat')
}

export function getRememberedChatRoute(): string | null {
  return normalizeStoredChatRoute(getLocalStorageItem(CHAT_LAST_ROUTE_KEY))
}

export function forgetRememberedChatRoute() {
  removeLocalStorageItem(CHAT_LAST_ROUTE_KEY)
}

export function getRememberedChatSize(): { width: number; height: number } {
  try {
    const raw = getLocalStorageItem(CHAT_WINDOW_SIZE_KEY)
    if (!raw) return CHAT_DEFAULT_SIZE
    const parsed = JSON.parse(raw) as Partial<{ width: number; height: number }>
    const width = Number(parsed.width)
    const height = Number(parsed.height)
    if (!Number.isFinite(width) || !Number.isFinite(height)) return CHAT_DEFAULT_SIZE
    return {
      width: Math.max(CHAT_MIN_SIZE.width, Math.round(width)),
      height: Math.max(CHAT_MIN_SIZE.height, Math.round(height)),
    }
  } catch {
    return CHAT_DEFAULT_SIZE
  }
}

export function rememberChatSize(width: number, height: number) {
  const next = {
    width: Math.max(CHAT_MIN_SIZE.width, Math.round(width)),
    height: Math.max(CHAT_MIN_SIZE.height, Math.round(height)),
  }
  setLocalStorageItem(CHAT_WINDOW_SIZE_KEY, JSON.stringify(next))
}
