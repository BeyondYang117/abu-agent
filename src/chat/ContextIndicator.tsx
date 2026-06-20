import { RefreshCw, X, Archive } from 'lucide-react'
import { useMemo, useState } from 'react'
import { formatTokens } from '../utils/tokens'
import type { ConversationContextState, ContextUsageSegment } from './types'

interface ContextIndicatorProps {
  contextState?: ConversationContextState | null
  messageCount?: number
  loading?: boolean
  compressing?: boolean
  error?: string
  usesExternalRuntime?: boolean
  onRefresh?: () => void
  onCompress?: () => void
}

function valueFrom<T>(snake: T | undefined, camel: T | undefined, fallback: T): T {
  return snake ?? camel ?? fallback
}

function segmentTokens(segment: ContextUsageSegment): number {
  return segment.estimated_tokens ?? segment.estimatedTokens ?? 0
}

function compactPercent(ratio: number | null): string {
  if (ratio == null || !Number.isFinite(ratio)) return '--'
  return `${Math.max(0, Math.min(999, Math.round(ratio * 100)))}%`
}

function statusColor(status: string, ratio: number | null): string {
  if (status === 'stale') return '#A15C2F'
  if (status === 'compressed') return '#3E8B60'
  if (status === 'critical' || (ratio ?? 0) >= 0.95) return '#C24135'
  if (status === 'warning' || (ratio ?? 0) >= 0.7) return '#B7791F'
  return '#3E8B60'
}

function formatTokenTotal(tokens: number, exact = false): string {
  const formatted = formatTokens(tokens).replace('k', 'K')
  return exact ? formatted : `~${formatted}`
}

function formatTimestamp(seconds?: number | null): string {
  if (!seconds) return 'Never'
  return new Date(seconds * 1000).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function readableStatus(status: string): string {
  switch (status) {
    case 'compressed':
      return 'Compressed'
    case 'stale':
      return 'Summary stale'
    case 'critical':
      return 'Critical'
    case 'warning':
      return 'Warning'
    case 'normal':
      return 'Normal'
    default:
      return 'Estimated'
  }
}

export function ContextIndicator({
  contextState,
  messageCount = 0,
  loading = false,
  compressing = false,
  error = '',
  usesExternalRuntime = false,
  onRefresh,
  onCompress,
}: ContextIndicatorProps) {
  const [open, setOpen] = useState(false)
  const estimatedInputTokens = valueFrom(
    contextState?.estimated_input_tokens,
    contextState?.estimatedInputTokens,
    0,
  )
  const contextWindowTokens = valueFrom(
    contextState?.context_window_tokens,
    contextState?.contextWindowTokens,
    null,
  )
  const usageRatio = valueFrom(contextState?.usage_ratio, contextState?.usageRatio, null)
  const status = contextState?.status ?? 'unknown'
  const contextSource = valueFrom(contextState?.context_source, contextState?.contextSource, null)
  const tokenCountSource = valueFrom(
    contextState?.token_count_source,
    contextState?.tokenCountSource,
    null,
  )
  const isExternalContext =
    usesExternalRuntime || contextSource === 'external_cli'
  const isCliReported = tokenCountSource === 'cli_reported'
  const lastCompressedAt = valueFrom(
    contextState?.last_compressed_at,
    contextState?.lastCompressedAt,
    null,
  )
  const compressedMessageCount = valueFrom(
    contextState?.compressed_message_count,
    contextState?.compressedMessageCount,
    0,
  )
  const contextWarning = valueFrom(contextState?.warning, contextState?.warningMessage, null)
  const summary = contextState?.summary ?? null
  const color = statusColor(status, usageRatio)
  const percentLabel = loading ? '...' : compactPercent(usageRatio)
  const segments = useMemo(
    () => (contextState?.segments ?? []).filter((segment) => segmentTokens(segment) > 0),
    [contextState?.segments],
  )
  const fullness = usageRatio == null
    ? (isExternalContext ? 'CLI pending' : 'Estimated')
    : `${compactPercent(usageRatio)} Full`
  const windowLabel = contextWindowTokens
    ? `${formatTokens(contextWindowTokens).replace('k', 'K')} Tokens`
    : '? Tokens'
  const tokenLine = `${formatTokenTotal(estimatedInputTokens, isCliReported)} / ${windowLabel}`
  const sourceLabel = isExternalContext
    ? (isCliReported ? 'CLI reported' : 'CLI estimated')
    : 'Kivio estimated'
  const ringDegrees = usageRatio == null ? 0 : Math.max(0, Math.min(1, usageRatio)) * 360
  const canCompress = Boolean(onCompress) && !compressing && !loading && messageCount > 2
  const compressLabel = isExternalContext
    ? (compressing ? 'Compacting' : 'CLI Compact')
    : (compressing ? 'Compressing' : 'Compress')

  return (
    <div className="relative" data-tauri-drag-region="false">
      <button
        type="button"
        className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[12px] font-medium text-neutral-600 transition duration-[var(--kv-dur-instant)] hover:bg-black/[0.05] hover:text-neutral-900 active:scale-[0.97] dark:text-neutral-300 dark:hover:bg-white/[0.07] dark:hover:text-neutral-50"
        aria-label="Context"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span
          className="grid size-5 place-items-center rounded-full"
          style={{
            background: `conic-gradient(${color} ${ringDegrees}deg, rgba(120,120,120,.22) 0deg)`,
          }}
        >
          <span className="size-3 rounded-full bg-white dark:bg-[#212121]" />
        </span>
        <span className="chat-titlebar-context-label min-w-[2.2rem] text-left tabular-nums">{percentLabel}</span>
      </button>

      {open && (
        <div className="chat-motion-popover absolute right-0 top-9 z-40 w-[18rem] max-w-[calc(100vw-2rem)] rounded-xl border border-neutral-200/90 bg-white p-3 shadow-xl dark:border-neutral-700 dark:bg-neutral-900">
          <div className="mb-3 flex items-center justify-between gap-2">
            <div className="flex min-w-0 items-center gap-2">
              <span className="text-[13px] font-semibold leading-none text-neutral-900 dark:text-neutral-50">
                Context
              </span>
              <span
                className="shrink-0 rounded-full px-1.5 py-[2px] text-[10px] font-medium leading-none"
                style={{ color, backgroundColor: `${color}1F` }}
              >
                {readableStatus(status)}
              </span>
              {isExternalContext && (
                <span className="shrink-0 rounded-full bg-neutral-100 px-1.5 py-[2px] text-[10px] font-medium leading-none text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
                  CLI
                </span>
              )}
            </div>
            <button
              type="button"
              className="-mr-1 -mt-1 rounded-md p-1 text-neutral-400 hover:bg-neutral-100 hover:text-neutral-700 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
              aria-label="Close context panel"
              onClick={() => setOpen(false)}
            >
              <X size={14} />
            </button>
          </div>

          <div className="mb-3 flex items-end justify-between gap-3">
            <div className="text-[24px] font-semibold leading-none tracking-tight text-neutral-900 dark:text-neutral-50">
              {fullness}
            </div>
            <div className="shrink-0 text-right">
              <div className="text-[11px] tabular-nums text-neutral-500 dark:text-neutral-400">
                {tokenLine}
              </div>
              <div className="mt-0.5 text-[10px] text-neutral-400 dark:text-neutral-500">
                {sourceLabel}
              </div>
              {messageCount > 0 && (
                <div className="mt-0.5 text-[10px] tabular-nums text-neutral-400 dark:text-neutral-500">
                  {messageCount} msgs{compressedMessageCount > 0 ? ` · ${compressedMessageCount} cmp` : ''}
                </div>
              )}
            </div>
          </div>

          <div className="mb-2.5 flex h-2 overflow-hidden rounded-full bg-neutral-100 dark:bg-neutral-800">
            {segments.length === 0 ? (
              <div className="h-full w-full bg-neutral-300 dark:bg-neutral-700" />
            ) : (
              segments.map((segment) => {
                const tokens = segmentTokens(segment)
                const width = estimatedInputTokens > 0 ? Math.max(2, (tokens / estimatedInputTokens) * 100) : 0
                return (
                  <div
                    key={segment.id}
                    className="h-full"
                    style={{
                      width: `${width}%`,
                      backgroundColor: segment.color || '#7A7A7A',
                    }}
                  />
                )
              })
            )}
          </div>

          <div className="max-h-32 space-y-1 overflow-auto">
            {segments.map((segment) => (
              <div key={segment.id} className="flex items-center gap-2 text-[11px] leading-tight">
                <span
                  className="size-1.5 shrink-0 rounded-full"
                  style={{ backgroundColor: segment.color || '#7A7A7A' }}
                />
                <span className="min-w-0 flex-1 truncate text-neutral-600 dark:text-neutral-300">
                  {segment.label}
                </span>
                <span className="shrink-0 tabular-nums text-neutral-500 dark:text-neutral-400">
                  {formatTokenTotal(segmentTokens(segment), isCliReported)}
                </span>
              </div>
            ))}
          </div>

          {(lastCompressedAt || summary?.stale || contextWarning || error) && (
            <div className="mt-2.5 border-t border-neutral-100 pt-2.5 text-[10px] leading-snug text-neutral-500 dark:border-neutral-800 dark:text-neutral-400">
              {lastCompressedAt && (
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate">Last compressed</span>
                  <span className="shrink-0">{formatTimestamp(lastCompressedAt)}</span>
                </div>
              )}
              {summary?.stale && !isExternalContext && (
                <div className="mt-0.5 text-[#A15C2F] dark:text-[#E0A06E]">
                  Summary will be ignored until recompressed.
                </div>
              )}
              {contextWarning && (
                <div className="mt-0.5 text-[#A15C2F] dark:text-[#E0A06E]">
                  {contextWarning}
                </div>
              )}
              {error && (
                <div className="mt-0.5 text-[#C24135] dark:text-[#F08A80]">
                  {error}
                </div>
              )}
            </div>
          )}

          <div className="mt-2.5 flex justify-end gap-1.5">
            <button
              type="button"
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium text-neutral-600 hover:bg-neutral-100 disabled:opacity-50 dark:text-neutral-300 dark:hover:bg-neutral-800"
              aria-label="Refresh context"
              onClick={onRefresh}
              disabled={loading}
            >
              <RefreshCw size={12} className={loading ? 'animate-spin' : ''} />
              Refresh
            </button>
            <button
              type="button"
              className="inline-flex items-center gap-1 rounded-md bg-neutral-900 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-neutral-700 disabled:cursor-not-allowed disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-200"
              aria-label="Compress context"
              onClick={onCompress}
              disabled={!canCompress}
            >
              <Archive size={12} />
              {compressLabel}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
