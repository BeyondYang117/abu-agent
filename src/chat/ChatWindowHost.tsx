import { useEffect, useState, type ReactNode } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { usesNativeTitlebar } from './platform'

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

type ChatWindowHostProps = {
  children: ReactNode
}

/** Chat 专用窗口外壳：真实窗口边界与视觉边界一致，最大化时收起圆角。 */
export function ChatWindowHost({ children }: ChatWindowHostProps) {
  const [maximized, setMaximized] = useState(false)

  useEffect(() => {
    if (!isTauriRuntime() || usesNativeTitlebar) return

    let cancelled = false
    let unlisten: (() => void) | undefined

    const syncMaximized = async () => {
      try {
        const next = await getCurrentWindow().isMaximized()
        if (!cancelled) setMaximized(next)
      } catch {
        // ignore
      }
    }

    const setup = async () => {
      await syncMaximized()
      const handler = await getCurrentWindow().onResized(() => {
        void syncMaximized()
      })
      if (cancelled) {
        handler()
      } else {
        unlisten = handler
      }
    }

    void setup()
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  if (usesNativeTitlebar) {
    return <div className="h-full w-full">{children}</div>
  }

  return (
    <div
      className={`chat-window-host h-full w-full${maximized ? ' chat-window-host--maximized' : ''}`}
    >
      {children}
    </div>
  )
}
