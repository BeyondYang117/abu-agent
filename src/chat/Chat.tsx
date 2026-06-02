import { useState, useEffect } from 'react'
import { Sidebar } from './Sidebar'
import { MessageList } from './MessageList'
import { InputBar } from './InputBar'
import { ModelSelector } from './ModelSelector'
import { chatApi } from './api'
import { api } from '../api/tauri'
import type { Conversation } from './types'

interface ChatProps {
  onOpenSettings: () => void
}

export default function Chat({ onOpenSettings }: ChatProps) {
  const [currentConversation, setCurrentConversation] = useState<Conversation | null>(null)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [streaming, setStreaming] = useState(false)
  const [streamingContent, setStreamingContent] = useState('')

  // 监听流式响应事件
  useEffect(() => {
    let unlisten: (() => void) | undefined

    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen<any>('chat-stream', (event) => {
        const { kind, text } = event.payload

        if (kind === 'chunk') {
          setStreamingContent((prev) => prev + text)
        } else if (kind === 'done' || kind === 'error') {
          setStreaming(false)
          setStreamingContent('')
          // 重新加载对话以获取完整消息
          if (currentConversation) {
            reloadConversation(currentConversation.id)
          }
        }
      })
    }

    setupListener()
    return () => {
      unlisten?.()
    }
  }, [currentConversation])

  // 重新加载对话
  const reloadConversation = async (conversationId: string) => {
    try {
      const conv = await chatApi.getConversation(conversationId)
      setCurrentConversation(conv)
    } catch (err) {
      console.error('Failed to reload conversation:', err)
    }
  }

  // 选择对话
  const handleSelectConversation = async (conversationId: string) => {
    try {
      const conv = await chatApi.getConversation(conversationId)
      setCurrentConversation(conv)
    } catch (err) {
      console.error('Failed to load conversation:', err)
    }
  }

  // 新建对话
  const handleNewConversation = async () => {
    try {
      const conv = await chatApi.createConversation()
      setCurrentConversation(conv)
    } catch (err) {
      console.error('Failed to create conversation:', err)
    }
  }

  // 发送消息
  const handleSendMessage = async (content: string) => {
    if (!currentConversation || streaming) return

    setStreaming(true)
    setStreamingContent('')

    try {
      const updatedConv = await chatApi.sendMessage(currentConversation.id, content)
      setCurrentConversation(updatedConv)
    } catch (err) {
      console.error('Failed to send message:', err)
      setStreaming(false)
    }
  }

  // 切换模型
  const handleModelChange = async (providerId: string, model: string) => {
    if (!currentConversation) return

    try {
      // 更新当前对话的模型配置
      const updatedConv = {
        ...currentConversation,
        provider_id: providerId,
        model: model,
      }
      setCurrentConversation(updatedConv)

      // TODO: 保存到后端
    } catch (err) {
      console.error('Failed to change model:', err)
    }
  }

  // 触发截图
  const handleTriggerScreenshot = async () => {
    try {
      await api.lensRequest()
    } catch (err) {
      console.error('Failed to trigger screenshot:', err)
    }
  }

  return (
    <div className="flex h-screen bg-white dark:bg-neutral-900">
      {/* 左侧边栏 */}
      <Sidebar
        currentConversationId={currentConversation?.id}
        onSelectConversation={handleSelectConversation}
        onNewConversation={handleNewConversation}
        onOpenSettings={onOpenSettings}
        collapsed={sidebarCollapsed}
      />

      {/* 主内容区 */}
      <div className="flex-1 flex flex-col">
        {/* 顶部栏 */}
        {currentConversation && (
          <div className="h-14 border-b border-neutral-200 dark:border-neutral-800 flex items-center justify-between px-4">
            {/* 对话标题 */}
            <div className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
              {currentConversation.title}
            </div>

            {/* 模型选择器 */}
            <ModelSelector
              currentProviderId={currentConversation.provider_id}
              currentModel={currentConversation.model}
              onModelChange={handleModelChange}
            />
          </div>
        )}

        {/* 消息列表 */}
        <MessageList
          messages={currentConversation?.messages || []}
          streaming={streaming}
        />

        {/* 输入栏 */}
        {currentConversation && (
          <InputBar
            onSend={handleSendMessage}
            disabled={streaming}
            onTriggerScreenshot={handleTriggerScreenshot}
          />
        )}

        {/* 空状态（无对话选中） */}
        {!currentConversation && (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center">
              <h2 className="text-3xl font-medium text-neutral-900 dark:text-neutral-100 mb-4">
                今天我能为您做些什么？
              </h2>
              <button
                onClick={handleNewConversation}
                className="px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-xl font-medium transition-colors"
              >
                开始新对话
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
