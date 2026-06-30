import { useCallback, useSyncExternalStore } from 'react'

// 多模型一问多答（任务 06-30）：多答组的展示模式偏好。
//  - 'tabs'（切换，默认）：一次只整宽显示一条答案，组末尾 footer 切换显示哪条。
//  - 'columns'（并排）：N 列横向并排（原有实现）。
// 这是一个**全局 UI 偏好**：跨会话共用、重启保留，写在 localStorage，不进后端 settings。
// 同一窗口内多个订阅者（多个多答组的 footer）通过模块级 store 即时同步；其它窗口/标签页
// 通过 storage 事件同步。

export type MultiAnswerViewMode = 'tabs' | 'columns'

export const MULTI_ANSWER_VIEW_STORAGE_KEY = 'kivio.chat.multiAnswerView'

const DEFAULT_MODE: MultiAnswerViewMode = 'tabs'

function isValidMode(value: string | null): value is MultiAnswerViewMode {
  return value === 'tabs' || value === 'columns'
}

function readFromStorage(): MultiAnswerViewMode {
  if (typeof window === 'undefined') return DEFAULT_MODE
  try {
    const raw = window.localStorage.getItem(MULTI_ANSWER_VIEW_STORAGE_KEY)
    return isValidMode(raw) ? raw : DEFAULT_MODE
  } catch {
    // 隐私模式 / 存储被禁用 → 退回默认。
    return DEFAULT_MODE
  }
}

// 模块级缓存 + 订阅者集合：getSnapshot 必须返回稳定引用，故缓存当前值，仅在变化时更新。
let current: MultiAnswerViewMode = readFromStorage()
const subscribers = new Set<() => void>()

function emit() {
  for (const cb of subscribers) cb()
}

function setMode(next: MultiAnswerViewMode) {
  if (next === current) return
  current = next
  if (typeof window !== 'undefined') {
    try {
      window.localStorage.setItem(MULTI_ANSWER_VIEW_STORAGE_KEY, next)
    } catch {
      // 写失败（隐私模式）忽略：内存态仍生效到本会话窗口关闭。
    }
  }
  emit()
}

function subscribe(cb: () => void): () => void {
  subscribers.add(cb)
  // 跨窗口/标签页同步：另一窗口改了偏好 → storage 事件 → 重读并通知本窗口订阅者。
  const onStorage = (e: StorageEvent) => {
    if (e.key !== MULTI_ANSWER_VIEW_STORAGE_KEY) return
    const next = readFromStorage()
    if (next !== current) {
      current = next
      emit()
    }
  }
  if (typeof window !== 'undefined') {
    window.addEventListener('storage', onStorage)
  }
  return () => {
    subscribers.delete(cb)
    if (typeof window !== 'undefined') {
      window.removeEventListener('storage', onStorage)
    }
  }
}

function getSnapshot(): MultiAnswerViewMode {
  return current
}

/**
 * 多答组展示模式偏好（全局，跨会话）。返回 `[mode, setMode]`。
 * 默认 'tabs'（切换）。改动写 localStorage 并即时同步本窗口所有订阅者。
 */
export function useMultiAnswerViewMode(): [MultiAnswerViewMode, (mode: MultiAnswerViewMode) => void] {
  const mode = useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
  const set = useCallback((next: MultiAnswerViewMode) => setMode(next), [])
  return [mode, set]
}

// 测试辅助：直接重置内存态 + storage，避免用例间偏好串味。
export function _resetMultiAnswerViewModeForTest(): void {
  current = DEFAULT_MODE
  if (typeof window !== 'undefined') {
    try {
      window.localStorage.removeItem(MULTI_ANSWER_VIEW_STORAGE_KEY)
    } catch {
      // ignore
    }
  }
  emit()
}

// 测试辅助：直接置某模式（同步内存态 + storage + 通知订阅者）。
export function _setMultiAnswerViewModeForTest(mode: MultiAnswerViewMode): void {
  current = mode
  if (typeof window !== 'undefined') {
    try {
      window.localStorage.setItem(MULTI_ANSWER_VIEW_STORAGE_KEY, mode)
    } catch {
      // ignore
    }
  }
  emit()
}
