import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { ChevronDown, ChevronRight, FolderPlus, Plus } from 'lucide-react'
import { useT } from '../settings/i18n'
import type { ConversationMenuAnchor } from './ConversationContextMenu'
import { useCloseAnimation } from './useCloseAnimation'
import { useClampedMenuPosition } from './useClampedMenuPosition'

interface ProjectSectionMenuProps {
  anchor: ConversationMenuAnchor
  allCollapsed: boolean
  onAddExisting: () => void
  onCreateBlank: () => void
  onToggleCollapsed: () => void
  onClose: () => void
  triggerRef?: React.RefObject<HTMLElement | null>
}

export function ProjectSectionMenu({
  anchor,
  allCollapsed,
  onAddExisting,
  onCreateBlank,
  onToggleCollapsed,
  onClose: onCloseProp,
  triggerRef,
}: ProjectSectionMenuProps) {
  const t = useT()
  const menuRef = useRef<HTMLDivElement>(null)
  const pos = useClampedMenuPosition(menuRef, anchor)
  const { closing, startClose, onAnimationEnd } = useCloseAnimation(onCloseProp)
  const onClose = startClose

  useEffect(() => {
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node
      if (menuRef.current?.contains(target) || triggerRef?.current?.contains(target)) return
      onClose()
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('mousedown', onPointerDown)
    window.addEventListener('keydown', onKeyDown)
    return () => {
      window.removeEventListener('mousedown', onPointerDown)
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [onClose, triggerRef])

  return createPortal(
    <div
      ref={menuRef}
      className={`kv-menu ${closing ? 'chat-motion-popover-out' : 'chat-motion-popover chat-motion-menu-cascade'} fixed z-[200] min-w-[196px]`}
      style={{ left: pos.left, top: pos.top }}
      role="menu"
      onAnimationEnd={onAnimationEnd}
    >
      <button type="button" role="menuitem" className="kv-menu-item" onClick={() => { onAddExisting(); onClose() }}>
        <FolderPlus strokeWidth={1.75} />
        {t.chatAddExistingProjects}
      </button>
      <button type="button" role="menuitem" className="kv-menu-item" onClick={() => { onCreateBlank(); onClose() }}>
        <Plus strokeWidth={1.75} />
        {t.chatNewBlankProject}
      </button>
      <div className="my-1 border-t border-neutral-200/80 dark:border-neutral-700" />
      <button type="button" role="menuitem" className="kv-menu-item" onClick={() => { onToggleCollapsed(); onClose() }}>
        {allCollapsed ? <ChevronRight strokeWidth={1.75} /> : <ChevronDown strokeWidth={1.75} />}
        {allCollapsed ? t.chatExpandAllProjects : t.chatCollapseAllProjects}
      </button>
    </div>,
    document.body,
  )
}
