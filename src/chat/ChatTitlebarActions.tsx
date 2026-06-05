import { PanelLeftClose, PanelLeftOpen, SquarePen } from 'lucide-react'
import {
  chatTitlebarPillButtonClass,
  chatTitlebarPillIconClass,
} from './platform'

type ChatTitlebarActionsProps = {
  sidebarExpanded: boolean
  onToggleSidebar: () => void
  onNewConversation: () => void
}

export function ChatTitlebarActions({
  sidebarExpanded,
  onToggleSidebar,
  onNewConversation,
}: ChatTitlebarActionsProps) {
  const ToggleIcon = sidebarExpanded ? PanelLeftClose : PanelLeftOpen
  const toggleLabel = sidebarExpanded ? '收起侧栏' : '展开侧栏'

  return (
    <div className={chatTitlebarPillButtonClass} data-tauri-drag-region="false">
      <button
        type="button"
        onClick={onToggleSidebar}
        className={`${chatTitlebarPillIconClass} group`}
        title={toggleLabel}
        aria-label={toggleLabel}
      >
        <ToggleIcon
          size={15}
          strokeWidth={1.75}
          className={`transition-transform duration-300 ease-out will-change-transform group-hover:scale-110 group-active:scale-90 ${
            sidebarExpanded ? 'group-hover:-translate-x-0.5' : 'group-hover:translate-x-0.5'
          }`}
        />
      </button>
      <span
        aria-hidden
        className="h-4 w-px shrink-0 bg-neutral-200/90 dark:bg-neutral-700"
      />
      <button
        type="button"
        onClick={onNewConversation}
        className={`${chatTitlebarPillIconClass} group`}
        title="新建聊天"
        aria-label="新建聊天"
      >
        <SquarePen
          size={15}
          strokeWidth={1.75}
          className="transition-transform duration-300 ease-out will-change-transform group-hover:-rotate-6 group-hover:scale-110 group-active:scale-90"
        />
      </button>
    </div>
  )
}
