import type { ConversationGroup } from './types'
import { formatRelativeTime } from './utils'

interface ConversationListProps {
  groups: ConversationGroup[]
  currentConversationId?: string
  onSelectConversation: (id: string) => void
}

export function ConversationList({
  groups,
  currentConversationId,
  onSelectConversation,
}: ConversationListProps) {
  return (
    <div className="px-3 py-2">
      {groups.map((group) => (
        <div key={group.title} className="mb-4">
          {/* 分组标题 */}
          <div className="px-3 py-1.5 text-xs font-semibold text-neutral-500 dark:text-neutral-400">
            {group.title}
          </div>

          {/* 对话列表 */}
          {group.conversations.map((conv) => (
            <button
              key={conv.id}
              onClick={() => onSelectConversation(conv.id)}
              className={`w-full text-left px-3 py-2.5 rounded-lg mb-1 transition-colors ${
                currentConversationId === conv.id
                  ? 'bg-neutral-200 dark:bg-neutral-800'
                  : 'hover:bg-neutral-100 dark:hover:bg-neutral-800'
              }`}
            >
              <div className="text-sm font-medium text-neutral-900 dark:text-neutral-100 truncate mb-0.5">
                {conv.title}
              </div>
              <div className="text-xs text-neutral-500 dark:text-neutral-400 truncate">
                {conv.preview || '开始对话...'}
              </div>
              <div className="text-xs text-neutral-400 dark:text-neutral-500 mt-1">
                {formatRelativeTime(conv.updated_at)}
              </div>
            </button>
          ))}
        </div>
      ))}

      {groups.length === 0 && (
        <div className="text-center py-8 text-sm text-neutral-500">暂无对话</div>
      )}
    </div>
  )
}
