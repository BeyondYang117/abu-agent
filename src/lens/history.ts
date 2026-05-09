import type { HistoryItem } from './types'

export const HISTORY_MAX = 20
export const HISTORY_THUMB_SIZE = 96

const HISTORY_STORAGE_KEY = 'kivio:lens-history:v1'
const HISTORY_STORAGE_KEY_LEGACY = 'keylingo:lens-history:v1'

/** Canvas 缩放截图为小缩略图，避免历史记录把整张原图（几 MB）写进 localStorage */
export async function makeThumbnail(dataUrl: string, maxSize: number): Promise<string> {
  if (!dataUrl) return ''
  return new Promise((resolve) => {
    const img = new Image()
    img.onload = () => {
      const ratio = Math.min(maxSize / img.width, maxSize / img.height, 1)
      const w = Math.max(1, Math.round(img.width * ratio))
      const h = Math.max(1, Math.round(img.height * ratio))
      const canvas = document.createElement('canvas')
      canvas.width = w
      canvas.height = h
      const ctx = canvas.getContext('2d')
      if (!ctx) { resolve(dataUrl); return }
      ctx.drawImage(img, 0, 0, w, h)
      try { resolve(canvas.toDataURL('image/jpeg', 0.7)) }
      catch { resolve(dataUrl) }
    }
    img.onerror = () => resolve(dataUrl)
    img.src = dataUrl
  })
}

/** 从 localStorage 读历史。失败 / 损坏数据 → 空数组。
    一次性迁移：keylingo:lens-history:v1 → kivio:lens-history:v1 */
export function loadHistoryFromStorage(): HistoryItem[] {
  try {
    let raw = localStorage.getItem(HISTORY_STORAGE_KEY)
    if (!raw) {
      const legacy = localStorage.getItem(HISTORY_STORAGE_KEY_LEGACY)
      if (legacy) {
        localStorage.setItem(HISTORY_STORAGE_KEY, legacy)
        localStorage.removeItem(HISTORY_STORAGE_KEY_LEGACY)
        raw = legacy
      } else {
        return []
      }
    }
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.slice(0, HISTORY_MAX)
  } catch {
    return []
  }
}

/** 把历史写回 localStorage。失败时只 console.error 不抛（quota 满 / 隐私模式等） */
export function saveHistoryToStorage(history: HistoryItem[]) {
  try {
    localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(history))
  } catch (err) {
    console.error('[lens-history] localStorage save failed:', err)
  }
}
