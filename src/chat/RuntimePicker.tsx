import { useCallback, useEffect, useMemo, useState } from 'react'
import { ChevronDown } from 'lucide-react'
import { chatApi, type DetectedExternalAgent } from './api'
import { chatTitlebarPillButtonClass } from './platform'
import type { AgentRuntimeConfig } from './types'

interface RuntimePickerProps {
  agentRuntime: AgentRuntimeConfig
  onRuntimeChange: (runtime: AgentRuntimeConfig) => void
}

const BUILTIN: AgentRuntimeConfig = {
  kind: 'builtin',
  externalAgentId: null,
  externalModel: null,
  externalReasoning: null,
}

function externalRuntime(agentId: string, model?: string | null): AgentRuntimeConfig {
  return {
    kind: 'external',
    externalAgentId: agentId,
    externalModel: model ?? 'default',
    externalReasoning: null,
  }
}

export function RuntimePicker({ agentRuntime, onRuntimeChange }: RuntimePickerProps) {
  const [open, setOpen] = useState(false)
  const [agents, setAgents] = useState<DetectedExternalAgent[]>([])

  const loadAgents = useCallback(async () => {
    try {
      const list = await chatApi.detectExternalAgents()
      setAgents(list)
    } catch (err) {
      console.error('Failed to detect external agents:', err)
      setAgents([])
    }
  }, [])

  useEffect(() => {
    void loadAgents()
  }, [loadAgents])

  const label = useMemo(() => {
    if (agentRuntime.kind !== 'external' || !agentRuntime.externalAgentId) {
      return '内置 Agent'
    }
    const agent = agents.find((item) => item.id === agentRuntime.externalAgentId)
    const name = agent?.name ?? agentRuntime.externalAgentId
    const model = agentRuntime.externalModel && agentRuntime.externalModel !== 'default'
      ? ` · ${agentRuntime.externalModel}`
      : ''
    return `${name}${model}`
  }, [agentRuntime, agents])

  const selectBuiltin = () => {
    onRuntimeChange(BUILTIN)
    setOpen(false)
  }

  const selectExternal = (agent: DetectedExternalAgent) => {
    if (!agent.available) return
    const defaultModel = agent.models[0]?.id ?? 'default'
    onRuntimeChange(externalRuntime(agent.id, defaultModel))
    setOpen(false)
  }

  return (
    <div className="relative max-w-full min-w-0" data-tauri-drag-region="false">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={`${chatTitlebarPillButtonClass} max-w-full min-w-0`}
        title="切换 Agent 运行时"
      >
        <span className="max-w-[160px] truncate font-medium text-neutral-800 dark:text-neutral-200">
          {label}
        </span>
        <ChevronDown
          size={15}
          className={`shrink-0 text-neutral-400 transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} aria-hidden />
          <div className="chat-model-selector-menu chat-motion-popover absolute left-0 top-full z-20 mt-2 min-w-[220px] overflow-y-auto rounded-2xl border border-neutral-200/90 bg-white py-1 shadow-lg dark:border-neutral-700 dark:bg-neutral-900">
            <button
              type="button"
              onClick={selectBuiltin}
              className={`flex w-full px-3 py-2 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800 ${
                agentRuntime.kind === 'builtin' ? 'font-semibold text-neutral-900 dark:text-neutral-100' : 'text-neutral-700 dark:text-neutral-300'
              }`}
            >
              内置 Agent
            </button>
            <div className="my-1 border-t border-neutral-100 dark:border-neutral-800" />
            <div className="px-3 py-1 text-[11px] font-semibold uppercase tracking-wide text-neutral-400">
              外部 CLI
            </div>
            {agents.length === 0 ? (
              <div className="px-3 py-2 text-xs text-neutral-500">正在检测本机 CLI…</div>
            ) : (
              agents.map((agent) => (
                <button
                  key={agent.id}
                  type="button"
                  disabled={!agent.available}
                  title={
                    agent.available
                      ? agent.version ?? undefined
                      : `${agent.name} 未安装或未在 PATH 中`
                  }
                  onClick={() => selectExternal(agent)}
                  className={`flex w-full flex-col px-3 py-2 text-left text-sm hover:bg-neutral-100 disabled:cursor-not-allowed disabled:opacity-45 dark:hover:bg-neutral-800 ${
                    agentRuntime.externalAgentId === agent.id
                      ? 'font-semibold text-neutral-900 dark:text-neutral-100'
                      : 'text-neutral-700 dark:text-neutral-300'
                  }`}
                >
                  <span>{agent.name}</span>
                  {agent.available && agent.version ? (
                    <span className="text-[11px] text-neutral-400">{agent.version}</span>
                  ) : null}
                </button>
              ))
            )}
          </div>
        </>
      )}
    </div>
  )
}

interface ExternalModelSelectorProps {
  agentRuntime: AgentRuntimeConfig
  onModelChange: (model: string, reasoning?: string | null) => void
}

export function ExternalModelSelector({
  agentRuntime,
  onModelChange,
}: ExternalModelSelectorProps) {
  const [open, setOpen] = useState(false)
  const [agents, setAgents] = useState<DetectedExternalAgent[]>([])

  useEffect(() => {
    void chatApi.detectExternalAgents().then(setAgents).catch(() => setAgents([]))
  }, [])

  const agent = agents.find((item) => item.id === agentRuntime.externalAgentId)
  const models = agent?.models ?? [{ id: 'default', label: 'Default' }]
  const reasoningOptions = agent?.reasoningOptions ?? []
  const displayName = agentRuntime.externalModel || 'default'

  if (agentRuntime.kind !== 'external' || !agentRuntime.externalAgentId) {
    return null
  }

  return (
    <div className="relative max-w-full min-w-0" data-tauri-drag-region="false">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={`${chatTitlebarPillButtonClass} max-w-full min-w-0`}
      >
        <span className="max-w-[140px] truncate font-medium text-neutral-800 dark:text-neutral-200">
          {displayName}
        </span>
        <ChevronDown
          size={15}
          className={`shrink-0 text-neutral-400 transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} aria-hidden />
          <div className="chat-model-selector-menu chat-motion-popover absolute left-0 top-full z-20 mt-2 max-h-[min(320px,50vh)] min-w-[200px] overflow-y-auto rounded-2xl border border-neutral-200/90 bg-white py-1 shadow-lg dark:border-neutral-700 dark:bg-neutral-900">
            {models.map((model) => (
              <button
                key={model.id}
                type="button"
                onClick={() => {
                  onModelChange(model.id)
                  setOpen(false)
                }}
                className={`block w-full px-3 py-2 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800 ${
                  displayName === model.id ? 'font-semibold' : ''
                }`}
              >
                {model.label}
              </button>
            ))}
            {reasoningOptions.length > 0 && (
              <>
                <div className="my-1 border-t border-neutral-100 dark:border-neutral-800" />
                <div className="px-3 py-1 text-[11px] font-semibold uppercase tracking-wide text-neutral-400">
                  Reasoning
                </div>
                {reasoningOptions.map((option) => (
                  <button
                    key={option.id}
                    type="button"
                    onClick={() => {
                      onModelChange(agentRuntime.externalModel ?? 'default', option.id)
                      setOpen(false)
                    }}
                    className="block w-full px-3 py-2 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800"
                  >
                    {option.label}
                  </button>
                ))}
              </>
            )}
          </div>
        </>
      )}
    </div>
  )
}
