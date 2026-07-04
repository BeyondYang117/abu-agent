import { useMemo, useState } from 'react'
import { Eye, EyeOff, ExternalLink, Info } from 'lucide-react'
import { api, type Settings } from '../api/tauri'
import type { I18n, Lang } from './i18n'
import { Input, Select, SettingRow, SettingsGroup, Toggle } from './components'

type WebSearchConfig = NonNullable<Settings['lens']['webSearch']>
/** 后端已接入的搜索源（settings 的 provider 枚举值）。 */
type ProviderId = 'tavily' | 'exa' | 'exa_mcp'

type WebSearchPanelProps = {
  t: I18n
  lang: Lang
  webSearch: WebSearchConfig | undefined
  onChange: (updates: Partial<WebSearchConfig>) => void
}

const DEFAULT_WEB_SEARCH: WebSearchConfig = {
  enabled: false,
  provider: 'tavily',
  tavilyApiKey: '',
  exaApiKey: '',
  exaMcpUrl: 'https://mcp.exa.ai/mcp',
  maxResults: 5,
  searchDepth: 'basic',
}

type ProviderDef = {
  id: ProviderId
  name: string
  site: string
  apiKeyUrl?: string
  /** 使用哪个密钥字段；ExaMCP 无需密钥则留空。 */
  keyField?: 'tavilyApiKey' | 'exaApiKey'
  /** 只读展示的固定 endpoint。 */
  endpoint?: string
  /** 可编辑 endpoint（写入 exaMcpUrl）。 */
  editableEndpoint?: boolean
  supportsDepth?: boolean
}

const PROVIDERS: ProviderDef[] = [
  {
    id: 'tavily',
    name: 'Tavily',
    site: 'https://tavily.com',
    apiKeyUrl: 'https://app.tavily.com/home',
    keyField: 'tavilyApiKey',
    endpoint: 'https://api.tavily.com',
    supportsDepth: true,
  },
  {
    id: 'exa',
    name: 'Exa',
    site: 'https://exa.ai',
    apiKeyUrl: 'https://dashboard.exa.ai/api-keys',
    keyField: 'exaApiKey',
    endpoint: 'https://api.exa.ai',
  },
  {
    id: 'exa_mcp',
    name: 'ExaMCP',
    site: 'https://docs.exa.ai/reference/exa-mcp',
    // Exa MCP 无需 API Key，只需 endpoint。
    editableEndpoint: true,
  },
]

/** 中性文字标：灰底 + 首字母，避免山寨各家品牌 logo。 */
function ProviderMark({ name, size = 20 }: { name: string; size?: number }) {
  return (
    <span
      className="grid shrink-0 place-items-center rounded-[6px] bg-zinc-100 font-semibold uppercase text-zinc-500 dark:bg-zinc-800 dark:text-zinc-300"
      style={{ width: size, height: size, fontSize: size * 0.46, lineHeight: 1 }}
    >
      {name.slice(0, 1)}
    </span>
  )
}

function hostname(url: string): string {
  return url.replace(/^https?:\/\//, '').replace(/\/.*$/, '')
}

function ApiKeyField({
  value,
  placeholder,
  onChange,
}: {
  value: string
  placeholder: string
  onChange: (value: string) => void
}) {
  const [reveal, setReveal] = useState(false)
  return (
    <div className="relative w-full">
      <Input
        type={reveal ? 'text' : 'password'}
        value={value}
        onChange={onChange}
        placeholder={placeholder}
        mono
        className="w-full pr-9"
      />
      <button
        type="button"
        onClick={() => setReveal((v) => !v)}
        className="absolute right-2 top-1/2 -translate-y-1/2 text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200"
        data-tauri-drag-region="false"
        aria-label={reveal ? 'Hide' : 'Show'}
      >
        {reveal ? <EyeOff size={15} /> : <Eye size={15} />}
      </button>
    </div>
  )
}

export function WebSearchPanel({ t, lang, webSearch, onChange }: WebSearchPanelProps) {
  const config = { ...DEFAULT_WEB_SEARCH, ...(webSearch ?? {}) }
  const [selectedId, setSelectedId] = useState<ProviderId>(config.provider)

  const selected = useMemo(
    () => PROVIDERS.find((p) => p.id === selectedId) ?? PROVIDERS[0],
    [selectedId],
  )

  const isDefault = selected.id === config.provider
  const keyValue = selected.keyField ? config[selected.keyField] || '' : ''

  return (
    <div className="flex min-h-full items-stretch gap-0">
      {/* 左侧二级侧边栏：搜索服务商 */}
      <nav className="relative flex h-full min-h-full w-44 shrink-0 flex-col self-stretch pr-3">
        <div
          className="pointer-events-none absolute inset-y-0 right-0 w-px bg-zinc-200/80 dark:bg-zinc-800"
          aria-hidden
        />
        <div className="px-2 pb-1 pt-1 text-[11px] font-medium uppercase tracking-wide text-zinc-400">
          {t.webSearchApiSection}
        </div>
        <div className="space-y-0.5">
          {PROVIDERS.map((provider) => {
            const active = provider.id === selectedId
            const marked = provider.id === config.provider
            return (
              <button
                key={provider.id}
                type="button"
                onClick={() => setSelectedId(provider.id)}
                className={`flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-sm transition ${
                  active
                    ? 'bg-indigo-50 font-medium text-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-300'
                    : 'text-zinc-700 hover:bg-zinc-100 dark:text-zinc-200 dark:hover:bg-zinc-800'
                }`}
                data-tauri-drag-region="false"
              >
                <ProviderMark name={provider.name} size={20} />
                <span className="min-w-0 flex-1 truncate">{provider.name}</span>
                {marked && (
                  <span className="shrink-0 rounded-full border border-emerald-400/60 px-1.5 py-px text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
                    {t.webSearchDefaultBadge}
                  </span>
                )}
              </button>
            )
          })}
        </div>
      </nav>

      {/* 右侧内容 */}
      <div className="min-w-0 flex-1 pl-5">
        {/* Hero 头部：名称 + 外链 + 默认控制 */}
        <div className="mb-5 flex items-center gap-3 border-b border-zinc-200/70 pb-4 dark:border-zinc-800">
          <ProviderMark name={selected.name} size={32} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="text-[16px] font-semibold text-zinc-800 dark:text-zinc-100">
                {selected.name}
              </span>
              <button
                type="button"
                onClick={() => void api.openExternal(selected.site)}
                className="text-zinc-400 hover:text-indigo-500"
                data-tauri-drag-region="false"
                aria-label="Open site"
              >
                <ExternalLink size={13} />
              </button>
            </div>
            <div className="truncate text-xs text-zinc-400">{hostname(selected.site)}</div>
          </div>
          <div className="ml-auto shrink-0">
            {isDefault ? (
              <span className="inline-flex items-center gap-1 rounded-full border border-emerald-400/50 bg-emerald-50 px-3 py-1 text-xs font-medium text-emerald-600 dark:bg-emerald-950/30 dark:text-emerald-400">
                {t.webSearchDefaultBadge}
              </span>
            ) : (
              <button
                type="button"
                onClick={() => onChange({ provider: selected.id })}
                className="kv-btn primary"
                data-tauri-drag-region="false"
              >
                {t.webSearchSetDefault}
              </button>
            )}
          </div>
        </div>

        <div className="space-y-6">
          <SettingsGroup title={t.webSearchProviderSection}>
            {selected.keyField && (
              <SettingRow label={t.lensWebSearchApiKey} stack>
                <div className="w-full space-y-1">
                  <ApiKeyField
                    value={keyValue}
                    placeholder={selected.keyField === 'exaApiKey' ? 'exa-...' : 'tvly-...'}
                    onChange={(value) => onChange({ [selected.keyField!]: value } as Partial<WebSearchConfig>)}
                  />
                  {selected.apiKeyUrl && (
                    <button
                      type="button"
                      onClick={() => void api.openExternal(selected.apiKeyUrl!)}
                      className="inline-flex items-center text-[12px] text-indigo-500 hover:underline dark:text-indigo-300"
                      data-tauri-drag-region="false"
                    >
                      {t.webSearchGetKey} ↗
                    </button>
                  )}
                </div>
              </SettingRow>
            )}

            <SettingRow
              label={t.webSearchApiUrl}
              description={selected.editableEndpoint ? t.webSearchExaMcpKeyless : undefined}
              stack
            >
              {selected.editableEndpoint ? (
                <Input
                  value={config.exaMcpUrl || ''}
                  onChange={(value) => onChange({ exaMcpUrl: value })}
                  placeholder="https://mcp.exa.ai/mcp"
                  mono
                  className="w-full"
                />
              ) : (
                <div className="w-full rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 font-mono text-[13px] text-zinc-500 dark:border-zinc-700 dark:bg-zinc-800/40 dark:text-zinc-400">
                  {selected.endpoint}
                </div>
              )}
            </SettingRow>

            {selected.supportsDepth && (
              <SettingRow label={t.lensWebSearchDepth}>
                <Select
                  className="w-44"
                  value={config.searchDepth || 'basic'}
                  onChange={(v) => onChange({ searchDepth: v as WebSearchConfig['searchDepth'] })}
                  options={[
                    { value: 'ultra-fast', label: 'Ultra fast' },
                    { value: 'fast', label: 'Fast' },
                    { value: 'basic', label: 'Basic' },
                    { value: 'advanced', label: 'Advanced' },
                  ]}
                />
              </SettingRow>
            )}

            {selected.keyField && !isDefault && (
              <p className="flex items-center gap-1.5 px-1 pt-1 text-[12px] text-zinc-400">
                <Info size={12} /> {t.webSearchApiKeyRequired}
              </p>
            )}
          </SettingsGroup>

          <SettingsGroup title={t.webSearchGeneralSection}>
            <SettingRow label={t.lensWebSearchMaxResults}>
              <Input
                type="number"
                min={1}
                max={10}
                className="w-24"
                value={String(config.maxResults ?? 5)}
                onChange={(value) => onChange({
                  maxResults: Math.min(10, Math.max(1, Number.parseInt(value, 10) || 1)),
                })}
              />
            </SettingRow>
          </SettingsGroup>

          <SettingsGroup title={t.webSearchLensSection}>
            <SettingRow label={t.enabled} description={t.lensWebSearchHint}>
              <Toggle
                checked={config.enabled === true}
                onChange={(v) => onChange({ enabled: v })}
              />
            </SettingRow>
          </SettingsGroup>

          <p className="kv-row-desc px-1">
            {lang === 'zh'
              ? 'Chat 联网开关在 MCP → 内置工具。'
              : 'Chat web toggles: MCP → Built-in tools.'}
          </p>
        </div>
      </div>
    </div>
  )
}
