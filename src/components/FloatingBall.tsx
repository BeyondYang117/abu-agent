import { useState, useEffect, type FormEvent } from 'react'
import { X, EyeOff, RotateCcw } from 'lucide-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { PhysicalPosition } from '@tauri-apps/api/dpi'
import { api, isTauriRuntime } from '../api/tauri'
import { ABUAgentBlob } from '../chat/AbuAgentBlob'
import './FloatingBall.css'

interface FloatingBallProps {
  themeMode?: 'light' | 'dark' | 'system'
}

export function FloatingBall({ themeMode = 'system' }: FloatingBallProps) {
  const [status, setStatus] = useState<'working' | 'done' | 'error'>('done')
  const [, setDetail] = useState('已完成')
  const [question, setQuestion] = useState('')
  const [expanded, setExpanded] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [dragging, setDragging] = useState(false)
  const [isDark, setIsDark] = useState(false)

  const clearDismissed = () => {
    window.localStorage.removeItem('abu-status-dismissed')
    window.localStorage.removeItem('abu-status-hidden')
  }

  const openChat = async () => {
    clearDismissed()
    await api.showChatWindow().catch(() => {})
    await api.setChatStatusIndicator(false).catch(() => {})
  }

  const hideForNow = async () => {
    window.localStorage.setItem('abu-status-hidden', '1')
    setMenuOpen(false)
    await api.setChatStatusIndicator(false).catch(() => {})
  }

  const exitFloating = async () => {
    window.localStorage.setItem('abu-status-dismissed', '1')
    window.localStorage.removeItem('abu-status-hidden')
    setMenuOpen(false)
    await api.setChatStatusIndicator(false).catch(() => {})
  }

  const startDrag = async (event: React.PointerEvent) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest('button, input')) return
    setDragging(true)
    try {
      await getCurrentWindow().startDragging()
    } finally {
      setDragging(false)
    }
  }

  const rememberPosition = async () => {
    try {
      const position = await getCurrentWindow().outerPosition()
      window.localStorage.setItem('abu-status-position', JSON.stringify({ x: position.x, y: position.y }))
    } catch {
      // Browser preview and early window teardown are harmless here.
    }
  }

  const submitQuestion = (event: FormEvent) => {
    event.preventDefault()
    if (!question.trim()) return openChat()
    void api.sendChatQuickQuestion(question.trim())
    setQuestion('')
    void openChat()
  }

  const handleContextMenu = (event: React.MouseEvent) => {
    event.preventDefault()
    setMenuOpen(!menuOpen)
  }

  // 监听主题变化
  useEffect(() => {
    const updateTheme = () => {
      const dark = themeMode === 'dark' ||
        (themeMode === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches) ||
        document.documentElement.classList.contains('dark')
      setIsDark(dark)
    }

    updateTheme()

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handleChange = () => updateTheme()
    mediaQuery.addEventListener('change', handleChange)

    const observer = new MutationObserver(() => updateTheme())
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class']
    })

    return () => {
      mediaQuery.removeEventListener('change', handleChange)
      observer.disconnect()
    }
  }, [themeMode])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let cancelled = false
    let unlisten: (() => void) | undefined
    api.onChatStream((payload) => {
      if (cancelled) return
      if (payload.type === 'run_started') {
        clearDismissed()
        setStatus('working')
        setDetail('正在处理…')
      } else if (payload.type === 'run_completed') {
        setStatus('done')
        setDetail('已完成')
      } else if (payload.type === 'run_failed') {
        setStatus('error')
        setDetail(`调用失败`)
      }
    }).then((dispose) => {
      if (cancelled) dispose()
      else unlisten = dispose
    }).catch(() => {})
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) return
    let unlisten: (() => void) | undefined
    void getCurrentWindow().onMoved(() => { void rememberPosition() }).then((dispose) => { unlisten = dispose }).catch(() => {})
    try {
      const saved = JSON.parse(window.localStorage.getItem('abu-status-position') || 'null')
      if (saved && Number.isFinite(saved.x) && Number.isFinite(saved.y)) {
        void getCurrentWindow().setPosition(new PhysicalPosition(saved.x, saved.y)).catch(() => {})
      }
    } catch {
      // Ignore malformed local preferences.
    }
    return () => unlisten?.()
  }, [])

  return (
    <div
      className={`modern-floating-ball${expanded ? ' is-expanded' : ''}${dragging ? ' is-dragging' : ''}${isDark ? ' dark-mode' : ' light-mode'}`}
      onPointerDown={startDrag}
      onDoubleClick={openChat}
      onPointerEnter={() => setExpanded(true)}
      onPointerLeave={() => !menuOpen && setExpanded(false)}
      onContextMenu={handleContextMenu}
    >
      {/* 主球体 */}
      <div className="ball-container">
        <div className={`ball-orb status-${status}`}>
          <ABUAgentBlob
            size={expanded ? 32 : 48}
            mood={status === 'working' ? 'think' : status === 'error' ? 'error' : 'idle'}
          />
        </div>
      </div>

      {/* 展开内容 */}
      {expanded && (
        <div className="ball-content" onClick={(e) => e.stopPropagation()}>
          <div className="ball-header">
            <div className="ball-title">
              <span className="ball-brand">ABU Agent</span>
              <span className={`ball-status-badge status-${status}`}>
                {status === 'working' ? '处理中' : status === 'error' ? '错误' : '就绪'}
              </span>
            </div>
          </div>

          <form className="ball-input-form" onSubmit={submitQuestion}>
            <input
              type="text"
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              placeholder="输入问题，回车发送..."
              className="ball-input"
              autoFocus
            />
          </form>
        </div>
      )}

      {/* 菜单下拉 */}
      {menuOpen && (
        <div className="ball-menu" onClick={(e) => e.stopPropagation()}>
          <button onClick={hideForNow}>
            <EyeOff size={14} />
            <span>隐藏</span>
          </button>
          <button onClick={exitFloating}>
            <X size={14} />
            <span>退出悬浮</span>
          </button>
          <button onClick={() => { clearDismissed(); setMenuOpen(false) }}>
            <RotateCcw size={14} />
            <span>恢复显示</span>
          </button>
        </div>
      )}
    </div>
  )
}
