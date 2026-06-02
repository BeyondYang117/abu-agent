import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { PanelLeftOpen } from 'lucide-react'
import { Sidebar } from './Sidebar'
import { MessageList, type AssistantStreamStats } from './MessageList'
import { InputBar } from './InputBar'
import { ModelSelector } from './ModelSelector'
import { WindowControls } from './WindowControls'
import { chatApi } from './api'
import { chatTitlebarMacInsetClass, chatTitlebarModelClass, chatTitlebarRowClass, usesNativeTitlebar } from './platform'
import type { ChatMessage, Conversation } from './types'
import { api } from '../api/tauri'
import { SettingsShell, type SettingsShellHandle } from '../settings/SettingsShell'
import { useWindowInteractionFocus } from '../utils/windowFocus'
import { estimateTokens } from '../lens/markdown'

type ChatView = 'conversation' | 'settings'

interface ChatProps {
  onSettingsChange: () => void
}

function hashPath(): string {
  return window.location.hash.replace('#', '').split('?')[0]
}

function isChatSettingsPath(path: string): boolean {
  return path === 'chat/settings' || path.startsWith('chat/settings/')
}

export default function Chat({ onSettingsChange }: ChatProps) {
  const [chatView, setChatView] = useState<ChatView>(() =>
    isChatSettingsPath(hashPath()) ? 'settings' : 'conversation',
  )
  const [currentConversation, setCurrentConversation] = useState<Awaited<
    ReturnType<typeof chatApi.getConversation>
  > | null>(null)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [searchOpen, setSearchOpen] = useState(false)
  const [streaming, setStreaming] = useState(false)
  const [streamingContent, setStreamingContent] = useState('')
  const [streamingReasoning, setStreamingReasoning] = useState('')
  const [streamError, setStreamError] = useState('')
  /** 发送中待显示的用户消息（与 conversation 分离，避免 route reload 冲掉） */
  const [pendingUserMessage, setPendingUserMessage] = useState<ChatMessage | null>(null)
  const [lastAssistantStreamStats, setLastAssistantStreamStats] =
    useState<AssistantStreamStats | null>(null)
  const [sidebarRefreshKey, setSidebarRefreshKey] = useState(0)
  const [draftProviderId, setDraftProviderId] = useState('')
  const [draftModel, setDraftModel] = useState('')
  const currentConversationIdRef = useRef<string | null>(null)
  const sendInFlightRef = useRef(false)
  const streamStartedAtRef = useRef<number | null>(null)
  const streamingContentRef = useRef('')
  const streamingReasoningRef = useRef('')
  const settingsRef = useRef<SettingsShellHandle>(null)
  const pendingAfterSettingsCloseRef = useRef<(() => void) | null>(null)
  const requestWindowFocus = useWindowInteractionFocus()

  const activeProviderId = currentConversation?.provider_id || draftProviderId
  const activeModel = currentConversation?.model || draftModel

  const getRouteConversationId = useCallback(() => {
    const path = hashPath()
    if (!path.startsWith('chat/')) return null
    const rest = path.slice('chat/'.length)
    if (rest === 'settings' || rest.startsWith('settings/')) return null
    return decodeURIComponent(rest)
  }, [])

  const syncConversationRoute = useCallback((conversationId: string | null) => {
    const nextHash = conversationId ? `#chat/${encodeURIComponent(conversationId)}` : '#chat'
    if (window.location.hash !== nextHash) {
      window.location.hash = nextHash
    }
  }, [])

  const syncSettingsRoute = useCallback(() => {
    if (window.location.hash !== '#chat/settings') {
      window.location.hash = '#chat/settings'
    }
  }, [])

  const refreshSidebar = useCallback(() => {
    setSidebarRefreshKey((key) => key + 1)
  }, [])

  const loadDefaultModel = useCallback(async () => {
    try {
      const settings = await api.getSettings()
      setDraftProviderId(settings.chatProviderId || settings.translatorProviderId || '')
      setDraftModel(settings.chatModel || settings.translatorModel || '')
    } catch {
      setDraftProviderId('dev-provider')
      setDraftModel('dev-model')
    }
  }, [])

  useEffect(() => {
    void loadDefaultModel()
  }, [loadDefaultModel])

  const openEmbeddedSettings = useCallback(() => {
    setChatView('settings')
    syncSettingsRoute()
  }, [syncSettingsRoute])

  const handleSettingsClose = useCallback(() => {
    setChatView('conversation')
    syncConversationRoute(currentConversationIdRef.current)
    const pending = pendingAfterSettingsCloseRef.current
    pendingAfterSettingsCloseRef.current = null
    pending?.()
  }, [syncConversationRoute])

  const runAfterLeavingSettings = useCallback((action: () => void) => {
    if (chatView !== 'settings') {
      action()
      return
    }
    pendingAfterSettingsCloseRef.current = action
    settingsRef.current?.requestClose()
  }, [chatView])

  const handleSettingsChange = useCallback(() => {
    onSettingsChange()
    void loadDefaultModel()
  }, [loadDefaultModel, onSettingsChange])

  const reloadConversation = useCallback(async (conversationId: string) => {
    if (sendInFlightRef.current) return
    try {
      const conv = await chatApi.getConversation(conversationId)
      setCurrentConversation(conv)
    } catch (err) {
      console.error('Failed to reload conversation:', err)
      setStreamError(typeof err === 'string' ? err : (err as Error).message || '对话加载失败')
    }
  }, [])

  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | undefined

    const setupListener = async () => {
      unlisten = await api.onChatStream((payload) => {
        if (cancelled) return
        if (payload.reasoningDelta) {
          streamingReasoningRef.current += payload.reasoningDelta
          setStreamingReasoning((prev) => prev + payload.reasoningDelta)
        }
        if (payload.delta) {
          streamingContentRef.current += payload.delta
          setStreamingContent((prev) => prev + payload.delta)
        }
        if (payload.done) {
          // invoke 未完成前不要清 streaming / reload：否则输入框会解锁，
          // 用户可在 regenerate/send 仍写盘时发新消息，导致旧助手回复被合并回来。
          if (sendInFlightRef.current) return

          setStreaming(false)
          setStreamingContent('')
          setStreamingReasoning('')
          if (payload.reason === 'error') {
            setStreamError('回复生成失败，请稍后重试。')
          }
          const conversationId = currentConversationIdRef.current
          if (conversationId && payload.reason !== 'cancelled') {
            void reloadConversation(conversationId)
            refreshSidebar()
          }
        }
      })
      if (cancelled) {
        unlisten()
      }
    }

    setupListener()
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [refreshSidebar, reloadConversation])

  useEffect(() => {
    currentConversationIdRef.current = currentConversation?.id ?? null
  }, [currentConversation?.id])

  useEffect(() => {
    let cleanup: (() => void) | undefined
    api.onOpenSettings(() => {
      const path = hashPath()
      if (!path.startsWith('chat')) return
      openEmbeddedSettings()
    }).then((unlisten) => {
      cleanup = unlisten
    })
    return () => {
      cleanup?.()
    }
  }, [openEmbeddedSettings])

  useEffect(() => {
    const loadFromRoute = () => {
      const path = hashPath()
      if (isChatSettingsPath(path)) {
        setChatView('settings')
        return
      }
      setChatView('conversation')
      if (sendInFlightRef.current) return
      const conversationId = getRouteConversationId()
      if (!conversationId) {
        setCurrentConversation(null)
        return
      }
      void reloadConversation(conversationId)
    }
    loadFromRoute()
    window.addEventListener('hashchange', loadFromRoute)
    return () => window.removeEventListener('hashchange', loadFromRoute)
  }, [getRouteConversationId, reloadConversation])

  const handleSelectConversation = async (conversationId: string) => {
    setLastAssistantStreamStats(null)
    try {
      const conv = await chatApi.getConversation(conversationId)
      setCurrentConversation(conv)
      syncConversationRoute(conversationId)
      setStreamError('')
    } catch (err) {
      console.error('Failed to load conversation:', err)
      setStreamError(typeof err === 'string' ? err : (err as Error).message || '对话加载失败')
    }
  }

  const handleNewConversation = useCallback(async () => {
    setLastAssistantStreamStats(null)
    try {
      const conv = await chatApi.createConversation(
        activeProviderId || undefined,
        activeModel || undefined
      )
      setCurrentConversation(conv)
      syncConversationRoute(conv.id)
      refreshSidebar()
      setStreamError('')
    } catch (err) {
      console.error('Failed to create conversation:', err)
      setStreamError(typeof err === 'string' ? err : (err as Error).message || '创建对话失败')
    }
  }, [activeModel, activeProviderId, refreshSidebar, syncConversationRoute])

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (chatView === 'settings') return
      const mod = e.metaKey || e.ctrlKey
      if (!mod) return
      if (e.key === 'n' || e.key === 'N') {
        e.preventDefault()
        void handleNewConversation()
      }
      if (e.key === 'k' || e.key === 'K') {
        e.preventDefault()
        setSearchOpen((open) => !open)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [chatView, handleNewConversation])

  const applyAssistantStreamStats = useCallback((updatedConv: Conversation) => {
    const lastAssistant = [...updatedConv.messages]
      .reverse()
      .find((message) => message.role === 'assistant')
    if (!lastAssistant || !streamStartedAtRef.current) return

    const elapsedSec = Math.max((Date.now() - streamStartedAtRef.current) / 1000, 0.1)
    const streamedText = `${streamingContentRef.current}${streamingReasoningRef.current ? `\n${streamingReasoningRef.current}` : ''}`
    const tokenEstimate = estimateTokens(
      streamedText.trim().length > 0
        ? streamedText
        : `${lastAssistant.content}${lastAssistant.reasoning ? `\n${lastAssistant.reasoning}` : ''}`,
    )
    setLastAssistantStreamStats({
      messageId: lastAssistant.id,
      tokensPerSec: tokenEstimate / elapsedSec,
    })
  }, [])

  const handleSendMessage = async (content: string) => {
    if (streaming || sendInFlightRef.current) return

    const trimmed = content.trim()
    if (!trimmed) return

    const pendingUserId = `pending-user-${Date.now()}`
    const optimisticUserMessage: ChatMessage = {
      id: pendingUserId,
      role: 'user',
      content: trimmed,
      timestamp: Math.floor(Date.now() / 1000),
    }

    setPendingUserMessage(optimisticUserMessage)
    setStreaming(true)
    setStreamingContent('')
    setStreamingReasoning('')
    setStreamError('')
    streamStartedAtRef.current = Date.now()
    streamingContentRef.current = ''
    streamingReasoningRef.current = ''

    sendInFlightRef.current = true
    try {
      let conversation = currentConversation
      if (!conversation) {
        conversation = await chatApi.createConversation(
          activeProviderId || undefined,
          activeModel || undefined
        )
        currentConversationIdRef.current = conversation.id
        setCurrentConversation(conversation)
        syncConversationRoute(conversation.id)
      }

      const updatedConv = await chatApi.sendMessage(conversation!.id, trimmed)
      applyAssistantStreamStats(updatedConv)
      setCurrentConversation(updatedConv)
      setPendingUserMessage(null)
      setStreaming(false)
      setStreamingContent('')
      setStreamingReasoning('')
      streamStartedAtRef.current = null
      streamingContentRef.current = ''
      streamingReasoningRef.current = ''
      refreshSidebar()
    } catch (err) {
      console.error('Failed to send message:', err)
      setPendingUserMessage(null)
      setStreaming(false)
      setStreamingContent('')
      setStreamingReasoning('')
      streamStartedAtRef.current = null
      streamingContentRef.current = ''
      streamingReasoningRef.current = ''
      setStreamError(typeof err === 'string' ? err : (err as Error).message || '发送失败')
    } finally {
      sendInFlightRef.current = false
    }
  }

  const handleUpdateMessage = useCallback(
    async (messageId: string, content: string) => {
      if (!currentConversation) return
      try {
        const updated = await chatApi.updateMessage(currentConversation.id, messageId, content)
        setCurrentConversation(updated)
        refreshSidebar()
      } catch (err) {
        console.error('Failed to update message:', err)
        setStreamError(typeof err === 'string' ? err : (err as Error).message || '保存失败')
      }
    },
    [currentConversation, refreshSidebar],
  )

  const handleDeleteMessage = useCallback(
    async (messageId: string) => {
      if (!currentConversation) return
      if (!window.confirm('确定删除这条助手回复吗？')) return
      try {
        const updated = await chatApi.deleteMessage(currentConversation.id, messageId)
        setCurrentConversation(updated)
        setLastAssistantStreamStats((prev) =>
          prev?.messageId === messageId ? null : prev,
        )
        refreshSidebar()
      } catch (err) {
        console.error('Failed to delete message:', err)
        setStreamError(typeof err === 'string' ? err : (err as Error).message || '删除失败')
      }
    },
    [currentConversation, refreshSidebar],
  )

  const handleRegenerateMessage = useCallback(
    async (messageId: string) => {
      if (!currentConversation || streaming || sendInFlightRef.current) return

      const conversationId = currentConversation.id
      const messageIndex = currentConversation.messages.findIndex(
        (message) => message.id === messageId,
      )
      if (messageIndex < 0) return

      setCurrentConversation({
        ...currentConversation,
        messages: currentConversation.messages.slice(0, messageIndex),
      })
      setLastAssistantStreamStats(null)
      setStreaming(true)
      setStreamingContent('')
      setStreamingReasoning('')
      setStreamError('')
      streamStartedAtRef.current = Date.now()
      streamingContentRef.current = ''
      streamingReasoningRef.current = ''
      sendInFlightRef.current = true

      try {
        const updated = await chatApi.regenerateMessage(conversationId, messageId)
        applyAssistantStreamStats(updated)
        setCurrentConversation(updated)
        refreshSidebar()
      } catch (err) {
        console.error('Failed to regenerate message:', err)
        setStreamError(typeof err === 'string' ? err : (err as Error).message || '重新生成失败')
        void reloadConversation(conversationId)
      } finally {
        setStreaming(false)
        setStreamingContent('')
        setStreamingReasoning('')
        streamStartedAtRef.current = null
        streamingContentRef.current = ''
        streamingReasoningRef.current = ''
        sendInFlightRef.current = false
      }
    },
    [applyAssistantStreamStats, currentConversation, refreshSidebar, reloadConversation, streaming],
  )

  const handleModelChange = async (providerId: string, model: string) => {
    setDraftProviderId(providerId)
    setDraftModel(model)

    if (!currentConversation) return

    try {
      const updatedConv = await chatApi.updateConversation(currentConversation.id, {
        providerId,
        model,
      })
      setCurrentConversation(updatedConv)
      refreshSidebar()
    } catch (err) {
      console.error('Failed to change model:', err)
      setStreamError(typeof err === 'string' ? err : (err as Error).message || '模型切换失败')
    }
  }

  const displayMessages = useMemo(() => {
    const stored = currentConversation?.messages ?? []
    if (!pendingUserMessage) return stored
    const alreadyStored = stored.some(
      (message) =>
        message.id === pendingUserMessage.id ||
        (message.role === 'user' &&
          message.content === pendingUserMessage.content &&
          message.timestamp >= pendingUserMessage.timestamp - 2),
    )
    return alreadyStored ? stored : [...stored, pendingUserMessage]
  }, [currentConversation?.messages, pendingUserMessage])

  const hasMessages = displayMessages.length > 0
  const showEmptyHero = chatView === 'conversation' && !hasMessages && !streaming && !streamError

  return (
    <div
      className={`chat-window-shell${usesNativeTitlebar ? ' chat-window-shell--native-titlebar' : ''}`}
      onPointerEnter={requestWindowFocus}
      onPointerMove={requestWindowFocus}
      onPointerDownCapture={requestWindowFocus}
    >
      <div className="flex h-full min-h-0 w-full">
        <Sidebar
          currentConversationId={currentConversation?.id}
          onSelectConversation={(id) => {
            runAfterLeavingSettings(() => void handleSelectConversation(id))
          }}
          onNewConversation={() => {
            runAfterLeavingSettings(() => void handleNewConversation())
          }}
          onConversationDeleted={() => {
            setCurrentConversation(null)
            syncConversationRoute(null)
            refreshSidebar()
          }}
          onOpenSettings={openEmbeddedSettings}
          settingsActive={chatView === 'settings'}
          collapsed={sidebarCollapsed}
          onToggleCollapsed={() => setSidebarCollapsed(true)}
          refreshKey={sidebarRefreshKey}
          searchOpen={searchOpen}
          onSearchOpenChange={(open) => {
            if (open) {
              runAfterLeavingSettings(() => setSearchOpen(true))
              return
            }
            setSearchOpen(false)
          }}
        />

        {chatView === 'settings' ? (
          <SettingsShell
            ref={settingsRef}
            variant="embedded"
            onClose={handleSettingsClose}
            onSettingsChange={handleSettingsChange}
          />
        ) : (
          <div className="relative flex min-w-0 flex-1 flex-col bg-white dark:bg-[#212121]">
            {sidebarCollapsed ? (
              <div
                className={`${chatTitlebarRowClass} ${chatTitlebarMacInsetClass} pr-4`}
                data-tauri-drag-region
              >
                {!usesNativeTitlebar && <WindowControls />}
                <button
                  type="button"
                  onClick={() => setSidebarCollapsed(false)}
                  className="rounded-md p-2 text-neutral-500 transition-colors hover:bg-black/[0.05] dark:hover:bg-white/[0.08]"
                  title="展开侧栏"
                  aria-label="展开侧栏"
                  data-tauri-drag-region="false"
                >
                  <PanelLeftOpen size={17} strokeWidth={1.75} />
                </button>
                <div className={chatTitlebarModelClass} data-tauri-drag-region="false">
                  <ModelSelector
                    currentProviderId={activeProviderId}
                    currentModel={activeModel}
                    onModelChange={(providerId, model) => void handleModelChange(providerId, model)}
                  />
                </div>
                <div className="min-w-0 flex-1" data-tauri-drag-region />
              </div>
            ) : (
              <header
                className={`${chatTitlebarRowClass} px-6`}
                data-tauri-drag-region
              >
                <div className={chatTitlebarModelClass} data-tauri-drag-region="false">
                  <ModelSelector
                    currentProviderId={activeProviderId}
                    currentModel={activeModel}
                    onModelChange={(providerId, model) => void handleModelChange(providerId, model)}
                  />
                </div>
                <div className="min-w-0 flex-1" data-tauri-drag-region />
              </header>
            )}

            <div className="flex min-h-0 flex-1 flex-col">
              {showEmptyHero ? (
                <div className="flex flex-1 flex-col items-center justify-center px-6">
                  <div className="w-full max-w-3xl space-y-8">
                    <h2 className="text-center text-[1.75rem] font-semibold leading-snug tracking-tight text-neutral-900 dark:text-neutral-50 sm:text-[2rem]">
                      今天我能为您做些什么？
                    </h2>
                    <InputBar
                      layout="inline"
                      onSend={(content) => void handleSendMessage(content)}
                      disabled={streaming || sendInFlightRef.current}
                      onOpenSettings={openEmbeddedSettings}
                      autoFocus
                    />
                  </div>
                </div>
              ) : (
                <>
                  <MessageList
                    messages={displayMessages}
                    streaming={streaming}
                    streamingContent={streamingContent}
                    streamingReasoning={streamingReasoning}
                    error={streamError}
                    lastAssistantStreamStats={lastAssistantStreamStats}
                    onUpdateMessage={handleUpdateMessage}
                    onRegenerateMessage={handleRegenerateMessage}
                    onDeleteMessage={handleDeleteMessage}
                  />
                  <InputBar
                    onSend={(content) => void handleSendMessage(content)}
                    disabled={streaming || sendInFlightRef.current}
                    onOpenSettings={openEmbeddedSettings}
                    autoFocus
                  />
                </>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
