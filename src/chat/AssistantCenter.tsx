import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  Bot,
  Copy,
  Play,
  Plus,
  RefreshCw,
  Save,
  Search,
  Trash2,
  X,
} from 'lucide-react'
import { api, type ModelProvider } from '../api/tauri'
import { isProviderEnabled } from '../settings/utils'
import { chatApi } from './api'
import type { ChatAssistant, AssistantToolPreset, SkillMeta } from './types'

interface AssistantCenterProps {
  skills: SkillMeta[]
  currentAssistantId?: string | null
  onStartAssistantChat: (assistant: ChatAssistant) => void
  onApplyAssistant?: (assistantId: string | null) => void
  onClose: () => void
}

type AssistantDraft = ChatAssistant

const toolPresetOptions: Array<{ value: AssistantToolPreset; label: string }> = [
  { value: 'inherit', label: '跟随聊天设置' },
  { value: 'none', label: '不使用工具' },
  { value: 'skills', label: '仅 Skill 工具' },
  { value: 'all', label: '全部可用工具' },
]

const assistantColors = ['#6A8FBD', '#C56646', '#4F9D7A', '#8A6FBD', '#B7791F', '#5E8C6A']

function nowSeconds() {
  return Math.floor(Date.now() / 1000)
}

function normalizeAssistantForDraft(assistant: ChatAssistant): AssistantDraft {
  return {
    ...assistant,
    description: assistant.description ?? '',
    icon: assistant.icon ?? 'bot',
    color: assistant.color ?? '#6A8FBD',
    system_prompt: assistant.system_prompt ?? assistant.systemPrompt ?? '',
    provider_id: assistant.provider_id ?? assistant.providerId ?? '',
    model: assistant.model ?? '',
    skill_id: assistant.skill_id ?? assistant.skillId ?? null,
    tool_preset: assistant.tool_preset ?? assistant.toolPreset ?? 'inherit',
    conversation_starters: assistant.conversation_starters ?? assistant.conversationStarters ?? [],
    greeting: assistant.greeting ?? '',
    enabled: assistant.enabled ?? true,
    archived: assistant.archived ?? false,
    built_in: assistant.built_in ?? assistant.builtIn ?? false,
    created_at: assistant.created_at ?? assistant.createdAt ?? nowSeconds(),
    updated_at: assistant.updated_at ?? assistant.updatedAt ?? nowSeconds(),
  }
}

function createBlankAssistant(): AssistantDraft {
  const now = nowSeconds()
  return {
    id: `asst_${crypto.randomUUID()}`,
    name: '新助手',
    description: '',
    icon: 'bot',
    color: '#6A8FBD',
    system_prompt: '',
    provider_id: '',
    model: '',
    skill_id: null,
    tool_preset: 'inherit',
    conversation_starters: [],
    greeting: '',
    enabled: true,
    archived: false,
    built_in: false,
    created_at: now,
    updated_at: now,
  }
}

function draftPayload(draft: AssistantDraft): ChatAssistant {
  return {
    ...draft,
    name: draft.name.trim(),
    description: draft.description?.trim() ?? '',
    icon: draft.icon?.trim() || 'bot',
    color: draft.color?.trim() || '#6A8FBD',
    system_prompt: (draft.system_prompt ?? draft.systemPrompt ?? '').trim(),
    provider_id: (draft.provider_id ?? draft.providerId ?? '').trim(),
    model: draft.provider_id ? (draft.model ?? '').trim() : '',
    skill_id: draft.skill_id || null,
    tool_preset: draft.tool_preset ?? 'inherit',
    conversation_starters: (draft.conversation_starters ?? [])
      .map((starter) => starter.trim())
      .filter(Boolean)
      .slice(0, 6),
    greeting: draft.greeting?.trim() ?? '',
    enabled: draft.enabled ?? true,
    archived: false,
    built_in: draft.built_in ?? false,
    created_at: draft.created_at,
    updated_at: nowSeconds(),
  }
}

function assistantMatches(assistant: ChatAssistant, query: string) {
  if (!query) return true
  const text = [
    assistant.name,
    assistant.description,
    assistant.system_prompt ?? assistant.systemPrompt,
    ...(assistant.conversation_starters ?? assistant.conversationStarters ?? []),
  ].join('\n').toLowerCase()
  return text.includes(query)
}

function providerModels(provider?: ModelProvider): string[] {
  if (!provider) return []
  return provider.enabledModels.length > 0 ? provider.enabledModels : provider.availableModels
}

export function AssistantCenter({
  skills,
  currentAssistantId,
  onStartAssistantChat,
  onApplyAssistant,
  onClose,
}: AssistantCenterProps) {
  const [assistants, setAssistants] = useState<ChatAssistant[]>([])
  const [providers, setProviders] = useState<ModelProvider[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(currentAssistantId ?? null)
  const [draft, setDraft] = useState<AssistantDraft | null>(null)
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  const loadAssistants = useCallback(async (preferredId?: string | null) => {
    setLoading(true)
    setError('')
    try {
      const data = await chatApi.getAssistants()
      setAssistants(data)
      const nextSelectedId = preferredId ?? selectedId ?? currentAssistantId ?? data[0]?.id ?? null
      const selected = data.find((assistant) => assistant.id === nextSelectedId) ?? data[0] ?? null
      setSelectedId(selected?.id ?? null)
      setDraft(selected ? normalizeAssistantForDraft(selected) : null)
    } catch (err) {
      setError(typeof err === 'string' ? err : (err as Error).message || '助手加载失败')
    } finally {
      setLoading(false)
    }
  }, [currentAssistantId, selectedId])

  const loadProviders = useCallback(async () => {
    try {
      const settings = await api.getSettings()
      setProviders(settings.providers || [])
    } catch {
      setProviders([])
    }
  }, [])

  useEffect(() => {
    void loadAssistants(currentAssistantId)
    void loadProviders()
  }, [currentAssistantId, loadAssistants, loadProviders])

  const filteredAssistants = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase()
    return assistants.filter((assistant) => assistantMatches(assistant, normalizedQuery))
  }, [assistants, query])

  const enabledProviders = useMemo(
    () => providers.filter(isProviderEnabled),
    [providers],
  )

  const selectedProvider = providers.find((provider) => provider.id === (draft?.provider_id ?? draft?.providerId))
  const models = providerModels(selectedProvider)
  const selectedSkill = skills.find((skill) => skill.id === draft?.skill_id)
  const canApplyCurrent = Boolean(currentAssistantId || onApplyAssistant)

  const updateDraft = <K extends keyof AssistantDraft>(key: K, value: AssistantDraft[K]) => {
    setDraft((prev) => (prev ? { ...prev, [key]: value } : prev))
  }

  const selectAssistant = (assistant: ChatAssistant) => {
    setSelectedId(assistant.id)
    setDraft(normalizeAssistantForDraft(assistant))
    setError('')
  }

  const handleCreate = () => {
    const blank = createBlankAssistant()
    setSelectedId(null)
    setDraft(blank)
    setError('')
  }

  const saveDraft = async (): Promise<ChatAssistant | null> => {
    if (!draft) return null
    const payload = draftPayload(draft)
    if (!payload.name) {
      setError('助手名称不能为空')
      return null
    }
    setSaving(true)
    setError('')
    try {
      const exists = assistants.some((assistant) => assistant.id === payload.id)
      const saved = exists
        ? await chatApi.updateAssistant(payload)
        : await chatApi.createAssistant(payload)
      await loadAssistants(saved.id)
      return saved
    } catch (err) {
      setError(typeof err === 'string' ? err : (err as Error).message || '助手保存失败')
      return null
    } finally {
      setSaving(false)
    }
  }

  const handleDuplicate = async () => {
    if (!draft || !assistants.some((assistant) => assistant.id === draft.id)) return
    setSaving(true)
    setError('')
    try {
      const copy = await chatApi.duplicateAssistant(draft.id)
      await loadAssistants(copy.id)
    } catch (err) {
      setError(typeof err === 'string' ? err : (err as Error).message || '复制失败')
    } finally {
      setSaving(false)
    }
  }

  const handleDelete = async () => {
    if (!draft) return
    const exists = assistants.some((assistant) => assistant.id === draft.id)
    if (!exists) {
      setDraft(null)
      setSelectedId(null)
      return
    }
    if (!window.confirm(`确定删除助手「${draft.name}」？已有对话会保留当时的助手快照。`)) return
    setSaving(true)
    setError('')
    try {
      await chatApi.deleteAssistant(draft.id)
      await loadAssistants(null)
    } catch (err) {
      setError(typeof err === 'string' ? err : (err as Error).message || '删除失败')
    } finally {
      setSaving(false)
    }
  }

  const handleStartChat = async () => {
    const saved = await saveDraft()
    if (saved) onStartAssistantChat(saved)
  }

  const handleApplyAssistant = async () => {
    const saved = await saveDraft()
    if (saved) onApplyAssistant?.(saved.id)
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-white text-neutral-900 dark:bg-[#212121] dark:text-neutral-100">
      <header className="flex h-[52px] shrink-0 items-center gap-2 border-b border-neutral-200/80 px-5 dark:border-neutral-800">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <Bot size={18} strokeWidth={1.8} className="shrink-0 text-neutral-500 dark:text-neutral-400" />
          <div className="min-w-0">
            <div className="truncate text-[14px] font-semibold">助手中心</div>
            <div className="truncate text-[11px] text-neutral-500 dark:text-neutral-400">
              {assistants.length} 个助手
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={() => void loadAssistants(selectedId)}
          className="grid h-8 w-8 place-items-center rounded-md text-neutral-500 hover:bg-neutral-100 hover:text-neutral-800 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
          aria-label="刷新助手"
          title="刷新"
        >
          <RefreshCw size={15} />
        </button>
        <button
          type="button"
          onClick={onClose}
          className="grid h-8 w-8 place-items-center rounded-md text-neutral-500 hover:bg-neutral-100 hover:text-neutral-800 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
          aria-label="关闭助手中心"
          title="关闭"
        >
          <X size={16} />
        </button>
      </header>

      <div className="assistant-center-layout flex min-h-0 flex-1">
        <aside className="assistant-center-sidebar flex w-[250px] shrink-0 flex-col border-r border-neutral-200/80 bg-[#f7f7f8] dark:border-neutral-800 dark:bg-[#1c1c1e]">
          <div className="space-y-2 p-3">
            <button
              type="button"
              onClick={handleCreate}
              className="flex h-9 w-full items-center justify-center gap-2 rounded-md bg-neutral-900 px-3 text-[13px] font-medium text-white hover:bg-neutral-700 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-200"
            >
              <Plus size={15} />
              新建助手
            </button>
            <div className="relative">
              <Search
                size={14}
                className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-neutral-400"
              />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="搜索助手"
                className="h-9 w-full rounded-md border border-neutral-200/90 bg-white pl-8 pr-3 text-[13px] outline-none placeholder:text-neutral-400 focus:border-neutral-300 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100"
              />
            </div>
          </div>

          <div className="custom-scrollbar min-h-0 flex-1 overflow-y-auto px-2 pb-3">
            {loading ? (
              <div className="px-3 py-8 text-center text-[13px] text-neutral-400">加载中...</div>
            ) : filteredAssistants.length === 0 ? (
              <div className="px-3 py-8 text-center text-[13px] text-neutral-400">没有匹配的助手</div>
            ) : (
              <div className="space-y-1">
                {filteredAssistants.map((assistant) => {
                  const active = selectedId === assistant.id
                  return (
                    <button
                      key={assistant.id}
                      type="button"
                      onClick={() => selectAssistant(assistant)}
                      className={`flex w-full min-w-0 items-start gap-2 rounded-md px-2.5 py-2 text-left transition-colors ${
                        active
                          ? 'bg-black/[0.06] text-neutral-900 dark:bg-white/[0.1] dark:text-neutral-50'
                          : 'text-neutral-700 hover:bg-black/[0.04] dark:text-neutral-300 dark:hover:bg-white/[0.06]'
                      }`}
                    >
                      <span
                        className="mt-0.5 grid size-7 shrink-0 place-items-center rounded-md text-[11px] font-semibold text-white"
                        style={{ backgroundColor: assistant.color || '#6A8FBD' }}
                      >
                        {assistant.name.trim().slice(0, 1) || '助'}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="flex min-w-0 items-center gap-1.5">
                          <span className="truncate text-[13px] font-medium">{assistant.name}</span>
                          {assistant.built_in && (
                            <span className="shrink-0 rounded bg-black/[0.06] px-1.5 py-0.5 text-[10px] text-neutral-500 dark:bg-white/[0.08] dark:text-neutral-400">
                              内置
                            </span>
                          )}
                        </span>
                        <span className="mt-0.5 line-clamp-2 text-[11px] leading-snug text-neutral-500 dark:text-neutral-400">
                          {assistant.description || '未设置描述'}
                        </span>
                      </span>
                    </button>
                  )
                })}
              </div>
            )}
          </div>
        </aside>

        <main className="custom-scrollbar min-h-0 flex-1 overflow-y-auto">
          {!draft ? (
            <div className="grid h-full place-items-center px-6 text-center text-[13px] text-neutral-400">
              暂无助手
            </div>
          ) : (
            <div className="mx-auto max-w-4xl px-6 py-5">
              {error && (
                <div className="mb-4 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
                  {error}
                </div>
              )}

              <div className="mb-5 flex min-w-0 items-start gap-3">
                <span
                  className="grid size-11 shrink-0 place-items-center rounded-lg text-[18px] font-semibold text-white"
                  style={{ backgroundColor: draft.color || '#6A8FBD' }}
                >
                  {draft.name.trim().slice(0, 1) || '助'}
                </span>
                <div className="min-w-0 flex-1">
                  <input
                    type="text"
                    value={draft.name}
                    onChange={(e) => updateDraft('name', e.target.value)}
                    className="w-full border-0 bg-transparent text-[22px] font-semibold leading-tight outline-none placeholder:text-neutral-300 dark:placeholder:text-neutral-600"
                    placeholder="助手名称"
                  />
                  <input
                    type="text"
                    value={draft.description ?? ''}
                    onChange={(e) => updateDraft('description', e.target.value)}
                    className="mt-1 w-full border-0 bg-transparent text-[13px] text-neutral-500 outline-none placeholder:text-neutral-400 dark:text-neutral-400"
                    placeholder="一句话描述"
                  />
                </div>
                <div className="flex shrink-0 gap-2">
                  <button
                    type="button"
                    onClick={() => void handleDuplicate()}
                    disabled={saving || !assistants.some((assistant) => assistant.id === draft.id)}
                    className="grid h-8 w-8 place-items-center rounded-md text-neutral-500 hover:bg-neutral-100 hover:text-neutral-800 disabled:cursor-not-allowed disabled:opacity-40 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
                    title="复制"
                    aria-label="复制助手"
                  >
                    <Copy size={15} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleDelete()}
                    disabled={saving}
                    className="grid h-8 w-8 place-items-center rounded-md text-neutral-500 hover:bg-red-50 hover:text-red-600 disabled:cursor-not-allowed disabled:opacity-40 dark:text-neutral-400 dark:hover:bg-red-950/30 dark:hover:text-red-300"
                    title="删除"
                    aria-label="删除助手"
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              </div>

              <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_18rem]">
                <section className="space-y-4">
                  <label className="block">
                    <span className="mb-1.5 block text-[12px] font-medium text-neutral-600 dark:text-neutral-300">
                      系统提示词
                    </span>
                    <textarea
                      value={draft.system_prompt ?? ''}
                      onChange={(e) => updateDraft('system_prompt', e.target.value)}
                      rows={10}
                      className="custom-scrollbar w-full resize-none rounded-md border border-neutral-200 bg-white px-3 py-2.5 text-[13px] leading-relaxed text-neutral-900 outline-none focus:border-neutral-300 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100"
                      placeholder="写下这个助手的角色、边界、输出风格和工作方式"
                    />
                  </label>

                  <label className="block">
                    <span className="mb-1.5 block text-[12px] font-medium text-neutral-600 dark:text-neutral-300">
                      开场白
                    </span>
                    <input
                      type="text"
                      value={draft.greeting ?? ''}
                      onChange={(e) => updateDraft('greeting', e.target.value)}
                      className="h-9 w-full rounded-md border border-neutral-200 bg-white px-3 text-[13px] outline-none focus:border-neutral-300 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100"
                      placeholder="进入助手聊天时显示的简短问候"
                    />
                  </label>

                  <label className="block">
                    <span className="mb-1.5 block text-[12px] font-medium text-neutral-600 dark:text-neutral-300">
                      开场问题
                    </span>
                    <textarea
                      value={(draft.conversation_starters ?? []).join('\n')}
                      onChange={(e) => updateDraft(
                        'conversation_starters',
                        e.target.value.split('\n').slice(0, 6),
                      )}
                      rows={5}
                      className="custom-scrollbar w-full resize-none rounded-md border border-neutral-200 bg-white px-3 py-2.5 text-[13px] leading-relaxed outline-none focus:border-neutral-300 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100"
                      placeholder="每行一个问题，最多 6 个"
                    />
                  </label>
                </section>

                <aside className="space-y-4">
                  <section className="space-y-3 rounded-md border border-neutral-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-900/60">
                    <div className="text-[12px] font-semibold text-neutral-700 dark:text-neutral-200">
                      运行设置
                    </div>
                    <label className="block">
                      <span className="mb-1 block text-[11px] text-neutral-500 dark:text-neutral-400">
                        模型供应商
                      </span>
                      <select
                        value={draft.provider_id ?? ''}
                        onChange={(e) => {
                          const providerId = e.target.value
                          const provider = providers.find((item) => item.id === providerId)
                          updateDraft('provider_id', providerId)
                          updateDraft('model', providerModels(provider)[0] ?? '')
                        }}
                        className="h-9 w-full rounded-md border border-neutral-200 bg-white px-2 text-[12px] outline-none dark:border-neutral-700 dark:bg-neutral-950 dark:text-neutral-100"
                      >
                        <option value="">跟随聊天默认</option>
                        {enabledProviders.map((provider) => (
                          <option key={provider.id} value={provider.id}>
                            {provider.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="block">
                      <span className="mb-1 block text-[11px] text-neutral-500 dark:text-neutral-400">
                        模型
                      </span>
                      <select
                        value={draft.model ?? ''}
                        disabled={!draft.provider_id}
                        onChange={(e) => updateDraft('model', e.target.value)}
                        className="h-9 w-full rounded-md border border-neutral-200 bg-white px-2 text-[12px] outline-none disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-950 dark:text-neutral-100"
                      >
                        {!draft.provider_id && <option value="">跟随聊天默认</option>}
                        {models.map((model) => (
                          <option key={model} value={model}>
                            {model}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="block">
                      <span className="mb-1 block text-[11px] text-neutral-500 dark:text-neutral-400">
                        Skill
                      </span>
                      <select
                        value={draft.skill_id ?? ''}
                        onChange={(e) => updateDraft('skill_id', e.target.value || null)}
                        className="h-9 w-full rounded-md border border-neutral-200 bg-white px-2 text-[12px] outline-none dark:border-neutral-700 dark:bg-neutral-950 dark:text-neutral-100"
                      >
                        <option value="">不绑定</option>
                        {skills.map((skill) => (
                          <option key={skill.id} value={skill.id}>
                            {skill.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="block">
                      <span className="mb-1 block text-[11px] text-neutral-500 dark:text-neutral-400">
                        工具策略
                      </span>
                      <select
                        value={draft.tool_preset ?? 'inherit'}
                        onChange={(e) => updateDraft('tool_preset', e.target.value)}
                        className="h-9 w-full rounded-md border border-neutral-200 bg-white px-2 text-[12px] outline-none dark:border-neutral-700 dark:bg-neutral-950 dark:text-neutral-100"
                      >
                        {toolPresetOptions.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="flex items-center justify-between gap-3 rounded-md bg-neutral-50 px-2.5 py-2 text-[12px] text-neutral-700 dark:bg-neutral-800/70 dark:text-neutral-200">
                      <span>启用助手</span>
                      <input
                        type="checkbox"
                        checked={draft.enabled !== false}
                        onChange={(e) => updateDraft('enabled', e.target.checked)}
                        className="size-4 accent-neutral-900 dark:accent-neutral-100"
                      />
                    </label>
                  </section>

                  <section className="space-y-3 rounded-md border border-neutral-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-900/60">
                    <div className="text-[12px] font-semibold text-neutral-700 dark:text-neutral-200">
                      标识
                    </div>
                    <div className="flex flex-wrap gap-1.5">
                      {assistantColors.map((color) => (
                        <button
                          key={color}
                          type="button"
                          onClick={() => updateDraft('color', color)}
                          className={`size-6 rounded-full border ${
                            draft.color === color
                              ? 'border-neutral-900 ring-2 ring-neutral-300 dark:border-neutral-100 dark:ring-neutral-600'
                              : 'border-transparent'
                          }`}
                          style={{ backgroundColor: color }}
                          aria-label={`选择颜色 ${color}`}
                        />
                      ))}
                    </div>
                    <input
                      type="text"
                      value={draft.icon ?? ''}
                      onChange={(e) => updateDraft('icon', e.target.value)}
                      className="h-9 w-full rounded-md border border-neutral-200 bg-white px-2 text-[12px] outline-none dark:border-neutral-700 dark:bg-neutral-950 dark:text-neutral-100"
                      placeholder="图标标识"
                    />
                  </section>

                  <section className="space-y-2 rounded-md border border-neutral-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-900/60">
                    <div className="text-[12px] font-semibold text-neutral-700 dark:text-neutral-200">
                      当前配置
                    </div>
                    <div className="space-y-1 text-[11px] text-neutral-500 dark:text-neutral-400">
                      <div className="truncate">模型：{draft.model || '跟随聊天默认'}</div>
                      <div className="truncate">Skill：{selectedSkill?.name || '不绑定'}</div>
                      <div className="truncate">工具：{toolPresetOptions.find((item) => item.value === draft.tool_preset)?.label ?? '跟随聊天设置'}</div>
                    </div>
                    {providers.length === 0 && (
                      <div className="rounded-md bg-amber-50 px-2 py-1.5 text-[11px] text-amber-700 dark:bg-amber-950/30 dark:text-amber-300">
                        未读取到模型供应商
                      </div>
                    )}
                  </section>
                </aside>
              </div>

              <footer className="sticky bottom-0 mt-5 flex items-center justify-between gap-3 border-t border-neutral-200 bg-white/95 py-3 backdrop-blur dark:border-neutral-800 dark:bg-[#212121]/95">
                <div className="min-w-0 truncate text-[11px] text-neutral-500 dark:text-neutral-400">
                  {draft.built_in ? '内置助手' : '自定义助手'} · {draft.enabled === false ? '已停用' : '可用'}
                </div>
                <div className="flex shrink-0 gap-2">
                  {canApplyCurrent && (
                    <button
                      type="button"
                      onClick={() => void handleApplyAssistant()}
                      disabled={saving || draft.enabled === false}
                      className="rounded-md px-3 py-1.5 text-[12px] font-medium text-neutral-600 hover:bg-neutral-100 disabled:cursor-not-allowed disabled:opacity-40 dark:text-neutral-300 dark:hover:bg-neutral-800"
                    >
                      应用到当前对话
                    </button>
                  )}
                  {currentAssistantId && onApplyAssistant && (
                    <button
                      type="button"
                      onClick={() => onApplyAssistant(null)}
                      disabled={saving}
                      className="rounded-md px-3 py-1.5 text-[12px] font-medium text-neutral-600 hover:bg-neutral-100 disabled:cursor-not-allowed disabled:opacity-40 dark:text-neutral-300 dark:hover:bg-neutral-800"
                    >
                      清除助手
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => void saveDraft()}
                    disabled={saving}
                    className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12px] font-medium text-neutral-600 hover:bg-neutral-100 disabled:cursor-not-allowed disabled:opacity-40 dark:text-neutral-300 dark:hover:bg-neutral-800"
                  >
                    <Save size={14} />
                    保存
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleStartChat()}
                    disabled={saving || draft.enabled === false}
                    className="flex items-center gap-1.5 rounded-md bg-neutral-900 px-3 py-1.5 text-[12px] font-medium text-white hover:bg-neutral-700 disabled:cursor-not-allowed disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-200"
                  >
                    <Play size={14} />
                    开始聊天
                  </button>
                </div>
              </footer>
            </div>
          )}
        </main>
      </div>
    </div>
  )
}
