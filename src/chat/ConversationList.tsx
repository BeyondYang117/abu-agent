import type { ConversationListItem } from './types'

interface ConversationListProps {
  conversations: ConversationListItem[]
  currentConversationId?: string
  onSelectConversation: (id: string) => void
}

export function ConversationList({
  conversations,
  currentConversationId,
  onSelectConversation,
}: ConversationListProps) {
  if (conversations.length === 0) {
    return (
      <div className="px-3 py-10 text-center text-[13px] text-neutral-400 dark:text-neutral-500">
        暂无对话
      </div>
    )
  }

  return (
    <div className="space-y-0.5 py-1">
      {conversations.map((conv) => {
        const active = currentConversationId === conv.id
        return (
          <button
            key={conv.id}
            type="button"
            onClick={() => onSelectConversation(conv.id)}
            className={`w-full truncate rounded-lg px-3 py-2 text-left text-[13px] transition-colors ${
              active
                ? 'bg-black/[0.06] font-medium text-neutral-900 dark:bg-white/[0.1] dark:text-neutral-100'
                : 'text-neutral-700 hover:bg-black/[0.04] dark:text-neutral-300 dark:hover:bg-white/[0.06]'
            }`}
            title={conv.title}
          >
            {conv.title}
          </button>
        )
      })}
    </div>
  )
}
