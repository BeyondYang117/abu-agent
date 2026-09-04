import { useState, useEffect, type FormEvent } from 'react'
import { X, EyeOff, RotateCcw } from 'lucide-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { api, isTauriRuntime } from '../api/tauri'
import { ABUAgentBlob } from '../chat/AbuAgentBlob'
import './ModernFloatingBall.css'

interface ModernFloatingBallProps {
  themeMode?: 'light' | 'dark' | 'system'
}

export function ModernFloatingBall({ themeMode = 'system' }: ModernFloatingBallProps) {
  const [status, setStatus] = useState<'idle' | 'working' | 'error'>('idle')
  const [expanded, setExpanded] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [question, setQuestion] = useState('')
  const [isDark, setIsDark] = useState(false)

  // 主题监听
  useEffect(() => {
    const updateTheme = () => {
      const htmlHasDark = document.documentElement.classList.contains('dark')
      const systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches

      let dark = false
      if (themeMode === 'dark') dark = true
      else if (themeMode === 'light') dark = false
      else if (themeMode === 'system') dark = systemDark

      if (htmlHasDark) dark = true

      setIsDark(dark)
    }

    updateTheme()

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    mediaQuery.addEventListener('change', updateTheme)

    const observer = new MutationObserver(updateTheme)
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class']
    })

    const interval = setInterval(updateTheme, 2000)

    return () => {
      mediaQuery.removeEventListener('change', updateTheme)
      observer.disconnect()
      clearInterval(interval)
    }
  }, [themeMode])

  // 监听聊天状态
  useEffect(() => {
    if (!isTauriRuntime()) return

    let unlisten: (() => void) | undefined

    api.onChatStream((payload) => {
      if (payload.type === 'run_started') {
        setStatus('working')
      } else if (payload.type === 'run_completed') {
        setStatus('idle')
      } else if (payload.type === 'run_failed') {
        setStatus('error')
      }
    }).then((dispose) => {
      unlisten = dispose
    }).catch(() => {})

    return () => unlisten?.()
  }, [])

  // 记住窗口位置
  useEffect(() => {
    if (!isTauriRuntime()) return

    let unlisten: (() => void) | undefined

    const rememberPosition = async () => {
      try {
        const position = await getCurrentWindow().outerPosition()
        localStorage.setItem('floating-ball-position', JSON.stringify(position))
      } catch {
        // The window may be closing while its final move event is delivered.
      }
    }

    getCurrentWindow().onMoved(rememberPosition).then((dispose) => {
      unlisten = dispose
    }).catch(() => {})

    // 恢复位置
    try {
      const saved = localStorage.getItem('floating-ball-position')
      if (saved) {
        const position = JSON.parse(saved)
        getCurrentWindow().setPosition(position).catch(() => {})
      }
    } catch {
      // Ignore malformed or stale saved positions.
    }

    return () => unlisten?.()
  }, [])

  const handleDrag = async (e: React.PointerEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button, input')) return
    try {
      await getCurrentWindow().startDragging()
    } catch {
      // Dragging is unavailable in browser previews.
    }
  }

  const handleDoubleClick = async () => {
    localStorage.removeItem('abu-status-dismissed')
    localStorage.removeItem('abu-status-hidden')
    await api.showChatWindow().catch(() => {})
    await api.setChatStatusIndicator(false).catch(() => {})
  }

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault()
    setMenuOpen(!menuOpen)
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (question.trim()) {
      api.sendChatQuickQuestion(question.trim())
      setQuestion('')
    }
    handleDoubleClick()
  }

  const hideFloating = async () => {
    localStorage.setItem('abu-status-hidden', '1')
    await api.setChatStatusIndicator(false).catch(() => {})
  }

  const exitFloating = async () => {
    localStorage.setItem('abu-status-dismissed', '1')
    await api.setChatStatusIndicator(false).catch(() => {})
  }

  const restoreFloating = () => {
    localStorage.removeItem('abu-status-dismissed')
    localStorage.removeItem('abu-status-hidden')
    setMenuOpen(false)
  }

  const statusText = status === 'working' ? '处理中' : status === 'error' ? '错误' : '就绪'

  return (
    <div
      className={`floating-ball-modern ${expanded ? 'expanded' : ''} ${isDark ? 'theme-dark' : 'theme-light'}`}
      onPointerDown={handleDrag}
      onDoubleClick={handleDoubleClick}
      onContextMenu={handleContextMenu}
      onPointerEnter={() => !menuOpen && setExpanded(true)}
      onPointerLeave={() => !menuOpen && setExpanded(false)}
    >
      {/* 球体 */}
      <div className={`ball-orb status-${status}`}>
        <ABUAgentBlob
          size={expanded ? 36 : 52}
          mood={status === 'working' ? 'think' : status === 'error' ? 'error' : 'idle'}
        />
      </div>

      {/* 展开内容 */}
      {expanded && (
        <div className="ball-expanded" onClick={(e) => e.stopPropagation()}>
          <div className="ball-header">
            <span className="ball-brand">ABU Agent</span>
            <span className={`ball-badge badge-${status}`}>{statusText}</span>
          </div>

          <form className="ball-form" onSubmit={handleSubmit}>
            <input
              type="text"
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              placeholder="输入问题..."
              className="ball-input"
            />
          </form>
        </div>
      )}

      {/* 右键菜单 */}
      {menuOpen && (
        <div className="ball-menu" onClick={(e) => e.stopPropagation()}>
          <button onClick={hideFloating}>
            <EyeOff size={14} />
            <span>隐藏</span>
          </button>
          <button onClick={exitFloating}>
            <X size={14} />
            <span>退出</span>
          </button>
          <button onClick={restoreFloating}>
            <RotateCcw size={14} />
            <span>恢复</span>
          </button>
        </div>
      )}
    </div>
  )
}
