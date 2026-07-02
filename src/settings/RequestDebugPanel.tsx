import { useCallback, useEffect, useMemo, useState } from 'react'
import { RefreshCw, Trash2, Download, Copy, Check } from 'lucide-react'
import { api, type RequestDebugRecord } from '../api/tauri'
import { SettingRow, SettingsGroup, Toggle } from './components'

type RequestDebugPanelProps = {
  lang: string
  /** 当前开关值（来自 chatTools.requestDebugEnabled）。 */
  enabled: boolean
  /** 切换开关：更新 chatTools.requestDebugEnabled（用户按保存后生效）。 */
  onToggleEnabled: (enabled: boolean) => void
}

function formatTime(seconds?: number | null, lang = 'zh') {
  if (!seconds) return '--'
  return new Date(seconds * 1000).toLocaleString(lang === 'zh' ? 'zh-CN' : 'en-US', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

function formatDuration(ms?: number | null) {
  const n = Number(ms ?? 0)
  if (!Number.isFinite(n) || n <= 0) return '--'
  if (n >= 1000) return `${(n / 1000).toFixed(1)}s`
  return `${Math.round(n)}ms`
}

function totalTokens(record: RequestDebugRecord) {
  const usage = record.response.usage
  if (!usage) return null
  return (
    usage.totalTokens ??
    ((usage.inputTokens ?? 0) + (usage.outputTokens ?? 0) || null)
  )
}

function prettyJson(value: unknown) {
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

function CopyButton({ text, lang }: { text: string; lang: string }) {
  const [copied, setCopied] = useState(false)
  const copy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 1400)
    } catch {
      /* clipboard unavailable — ignore */
    }
  }, [text])
  return (
    <button type="button" className="kv-btn sm" onClick={copy} data-tauri-drag-region="false">
      {copied ? <Check size={11} /> : <Copy size={11} />}
      {copied ? (lang === 'zh' ? '已复制' : 'Copied') : lang === 'zh' ? '复制' : 'Copy'}
    </button>
  )
}

function JsonBlock({ title, value, lang }: { title: string; value: unknown; lang: string }) {
  const text = typeof value === 'string' ? value : prettyJson(value)
  return (
    <div className="rounded-md border border-neutral-200 dark:border-neutral-800">
      <div className="flex items-center justify-between border-b border-neutral-200 px-3 py-1.5 dark:border-neutral-800">
        <span className="text-[11px] font-medium text-neutral-500 dark:text-neutral-400">{title}</span>
        <CopyButton text={text} lang={lang} />
      </div>
      <pre className="max-h-[320px] overflow-auto px-3 py-2 text-[11px] leading-relaxed text-neutral-700 dark:text-neutral-300 whitespace-pre-wrap break-all font-mono">
        {text}
      </pre>
    </div>
  )
}

export function RequestDebugPanel({ lang, enabled, onToggleEnabled }: RequestDebugPanelProps) {
  const zh = lang === 'zh'
  const [records, setRecords] = useState<RequestDebugRecord[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')

  const refresh = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const next = await api.getRequestDebugRecords()
      setRecords(next)
      setSelectedId((prev) => {
        if (prev && next.some((r) => r.id === prev)) return prev
        return next[0]?.id ?? null
      })
    } catch (err) {
      setError(zh ? `加载失败：${err}` : `Load failed: ${err}`)
    } finally {
      setLoading(false)
    }
  }, [zh])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const clearAll = useCallback(async () => {
    try {
      await api.clearRequestDebugRecords()
      setRecords([])
      setSelectedId(null)
    } catch (err) {
      setError(zh ? `清空失败：${err}` : `Clear failed: ${err}`)
    }
  }, [zh])

  const exportJson = useCallback(() => {
    const blob = new Blob([prettyJson(records)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    const stamp = new Date().toISOString().replace(/[:.]/g, '-')
    anchor.href = url
    anchor.download = `kivio-request-debug-${stamp}.json`
    document.body.appendChild(anchor)
    anchor.click()
    anchor.remove()
    URL.revokeObjectURL(url)
  }, [records])

  const selected = useMemo(
    () => records.find((r) => r.id === selectedId) ?? null,
    [records, selectedId],
  )

  return (
    <div className="flex flex-col gap-4">
      <SettingsGroup title={zh ? '请求调试' : 'Request debug'}>
        <SettingRow
          label={zh ? '记录 provider 请求' : 'Capture provider requests'}
          description={
            zh
              ? '开启后每次 provider 调用（chat/子agent + 翻译/截图/Lens）的请求与响应被记入内存（脱敏 key，最多 50 条，不落盘）。改动后请保存设置生效。'
              : 'When on, every provider call (chat/sub-agent + translate/screenshot/Lens) is captured in memory (keys masked, up to 50, never written to disk). Save settings to apply.'
          }
          stack
        >
          <Toggle checked={enabled} onChange={onToggleEnabled} />
        </SettingRow>
      </SettingsGroup>

      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className="kv-btn sm" onClick={() => void refresh()} data-tauri-drag-region="false">
          <RefreshCw size={11} className={loading ? 'animate-spin' : ''} />
          {zh ? '刷新' : 'Refresh'}
        </button>
        <button
          type="button"
          className="kv-btn sm"
          onClick={exportJson}
          disabled={records.length === 0}
          data-tauri-drag-region="false"
        >
          <Download size={11} />
          {zh ? '导出 JSON' : 'Export JSON'}
        </button>
        <button
          type="button"
          className="kv-btn sm"
          onClick={() => void clearAll()}
          disabled={records.length === 0}
          data-tauri-drag-region="false"
        >
          <Trash2 size={11} />
          {zh ? '清空' : 'Clear'}
        </button>
        <span className="text-[11px] text-neutral-500 dark:text-neutral-400">
          {zh ? `${records.length} 条记录` : `${records.length} records`}
        </span>
      </div>

      {error && <div className="text-[11px] text-red-500">{error}</div>}

      <div className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(240px,320px)_1fr]">
        <div className="flex max-h-[520px] flex-col overflow-auto rounded-md border border-neutral-200 dark:border-neutral-800">
          {records.length === 0 ? (
            <div className="px-3 py-6 text-center text-[11px] text-neutral-400">
              {enabled
                ? zh
                  ? '暂无记录。触发一次对话或翻译后刷新。'
                  : 'No records yet. Trigger a chat or translation, then refresh.'
                : zh
                  ? '未开启记录。打开上方开关并保存。'
                  : 'Capture is off. Turn on the toggle above and save.'}
            </div>
          ) : (
            records.map((record) => {
              const tokens = totalTokens(record)
              const active = record.id === selectedId
              return (
                <button
                  key={record.id}
                  type="button"
                  onClick={() => setSelectedId(record.id)}
                  data-tauri-drag-region="false"
                  className={`flex flex-col gap-0.5 border-b border-neutral-100 px-3 py-2 text-left last:border-b-0 dark:border-neutral-900 ${
                    active ? 'bg-neutral-100 dark:bg-neutral-800/60' : 'hover:bg-neutral-50 dark:hover:bg-neutral-900/40'
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-[12px] font-medium text-neutral-800 dark:text-neutral-100">
                      {record.operation}
                    </span>
                    <span
                      className={`shrink-0 text-[10px] font-medium ${
                        record.status === 'success' ? 'text-emerald-500' : 'text-red-500'
                      }`}
                    >
                      {record.status}
                    </span>
                  </div>
                  <div className="truncate text-[10px] text-neutral-500 dark:text-neutral-400">{record.model}</div>
                  <div className="flex items-center gap-2 text-[10px] text-neutral-400">
                    <span>{formatTime(record.createdAt, lang)}</span>
                    <span>·</span>
                    <span>{formatDuration(record.durationMs)}</span>
                    {tokens != null && (
                      <>
                        <span>·</span>
                        <span>{tokens} tok</span>
                      </>
                    )}
                  </div>
                </button>
              )
            })
          )}
        </div>

        <div className="flex flex-col gap-3">
          {selected ? (
            <>
              <div className="grid grid-cols-2 gap-x-4 gap-y-1 rounded-md border border-neutral-200 px-3 py-2 text-[11px] dark:border-neutral-800 sm:grid-cols-3">
                <Meta label={zh ? '供应商' : 'Provider'} value={`${selected.providerName} (${selected.providerId})`} />
                <Meta label={zh ? '模型' : 'Model'} value={selected.model} />
                <Meta label={zh ? '格式' : 'Format'} value={selected.apiFormat} />
                <Meta label={zh ? '来源' : 'Source'} value={selected.source} />
                <Meta label={zh ? '流式' : 'Stream'} value={selected.request.stream ? 'true' : 'false'} />
                <Meta label={zh ? '状态码' : 'Status'} value={String(selected.response.statusCode ?? '--')} />
                {selected.conversationId && (
                  <Meta label={zh ? '会话' : 'Conversation'} value={selected.conversationId} />
                )}
                {selected.response.finishReason && (
                  <Meta label={zh ? '结束原因' : 'Finish'} value={selected.response.finishReason} />
                )}
              </div>
              <JsonBlock
                title={zh ? '请求 Headers（已脱敏）' : 'Request headers (sanitized)'}
                value={selected.request.headers}
                lang={lang}
              />
              <JsonBlock title={zh ? '请求 Body' : 'Request body'} value={selected.request.body} lang={lang} />
              <JsonBlock title={zh ? '响应' : 'Response'} value={selected.response} lang={lang} />
            </>
          ) : (
            <div className="rounded-md border border-dashed border-neutral-200 px-3 py-10 text-center text-[11px] text-neutral-400 dark:border-neutral-800">
              {zh ? '选择左侧一条记录查看详情。' : 'Select a record on the left to view details.'}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col">
      <span className="text-[10px] text-neutral-400">{label}</span>
      <span className="truncate text-neutral-700 dark:text-neutral-200" title={value}>
        {value}
      </span>
    </div>
  )
}
