import { useCallback } from 'react'
import { Minus, Square, X } from 'lucide-react'
import { api } from '../api/tauri'

const isMac =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent)

type TrafficButton = 'close' | 'minimize' | 'maximize'

const trafficColors: Record<TrafficButton, string> = {
  close: '#ff5f57',
  minimize: '#febc2e',
  maximize: '#28c840',
}

export function WindowControls() {
  const handleClose = useCallback(() => {
    void api.closeWindow()
  }, [])

  const handleMinimize = useCallback(() => {
    void api.minimizeWindow()
  }, [])

  const handleMaximize = useCallback(() => {
    void api.toggleMaximizeWindow()
  }, [])

  if (!isMac) {
    return (
      <div className="chat-win-controls chat-win-controls--win" data-tauri-drag-region="false">
        <button type="button" className="chat-win-btn" onClick={handleMinimize} aria-label="最小化">
          <Minus size={14} strokeWidth={1.9} aria-hidden />
        </button>
        <button type="button" className="chat-win-btn" onClick={handleMaximize} aria-label="最大化">
          <Square size={12} strokeWidth={1.9} aria-hidden />
        </button>
        <button
          type="button"
          className="chat-win-btn chat-win-btn--close"
          onClick={handleClose}
          aria-label="关闭"
        >
          <X size={14} strokeWidth={2.1} aria-hidden />
        </button>
      </div>
    )
  }

  return (
    <div className="chat-traffic" data-tauri-drag-region="false">
      {(['close', 'minimize', 'maximize'] as TrafficButton[]).map((kind) => (
        <button
          key={kind}
          type="button"
          className={`chat-traffic-dot chat-traffic-dot--${kind}`}
          style={{ ['--dot-color' as string]: trafficColors[kind] }}
          onClick={
            kind === 'close'
              ? handleClose
              : kind === 'minimize'
                ? handleMinimize
                : handleMaximize
          }
          aria-label={kind === 'close' ? '关闭' : kind === 'minimize' ? '最小化' : '最大化'}
        />
      ))}
    </div>
  )
}
