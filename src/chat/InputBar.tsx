import { useState } from 'react'
import { Plus, Send, Camera } from 'lucide-react'

interface InputBarProps {
  onSend: (content: string) => void
  disabled?: boolean
  onTriggerScreenshot?: () => void
  autoFocus?: boolean
}

export function InputBar({ onSend, disabled, onTriggerScreenshot, autoFocus }: InputBarProps) {
  const [input, setInput] = useState('')

  const handleSend = () => {
    const trimmed = input.trim()
    if (!trimmed || disabled) return
    onSend(trimmed)
    setInput('')
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Enter 发送，Shift+Enter 换行
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  return (
    <div className="border-t border-neutral-200 dark:border-neutral-800 p-4 bg-white dark:bg-neutral-900">
      <div className="max-w-4xl mx-auto">
        <div className="flex items-end gap-2">
          {/* 附件按钮 */}
          <button
            type="button"
            disabled={disabled}
            className="p-2.5 rounded-lg hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            title="添加附件"
          >
            <Plus size={20} className="text-neutral-600 dark:text-neutral-400" />
          </button>

          {/* 截图按钮 */}
          {onTriggerScreenshot && (
            <button
              type="button"
              onClick={onTriggerScreenshot}
              disabled={disabled}
              className="p-2.5 rounded-lg hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              title="截图"
            >
              <Camera size={20} className="text-neutral-600 dark:text-neutral-400" />
            </button>
          )}

          {/* 输入框 */}
          <textarea
            autoFocus={autoFocus}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={disabled}
            placeholder="随便问我什么..."
            rows={1}
            className="flex-1 resize-none px-4 py-3 bg-neutral-50 dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-xl text-[15px] text-neutral-900 dark:text-neutral-100 placeholder-neutral-400 dark:placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 disabled:opacity-50 disabled:cursor-not-allowed"
            style={{
              maxHeight: '200px',
              height: 'auto',
              overflowY: input.split('\n').length > 3 ? 'auto' : 'hidden',
            }}
          />

          {/* 发送按钮 */}
          <button
            type="button"
            onClick={handleSend}
            disabled={!input.trim() || disabled}
            className="p-2.5 rounded-lg bg-blue-500 hover:bg-blue-600 disabled:bg-neutral-300 dark:disabled:bg-neutral-700 transition-colors disabled:cursor-not-allowed"
            title="发送"
          >
            <Send
              size={20}
              className={
                input.trim() && !disabled
                  ? 'text-white'
                  : 'text-neutral-400 dark:text-neutral-500'
              }
            />
          </button>
        </div>

        {/* 提示文字 */}
        <div className="mt-2 text-xs text-neutral-400 dark:text-neutral-500 text-center">
          按 Enter 发送，Shift + Enter 换行
        </div>
      </div>
    </div>
  )
}
