import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Check, ChevronDown, Sparkles, Star } from 'lucide-react'
import { type ModelProvider } from '../api/tauri'
import { getSettingsCached, setFavoriteModelsCached, subscribeSettings } from '../api/settingsCache'
import { useLang, useT } from '../settings/i18n'
import { isProviderEnabled } from '../settings/utils'
import { ModelIcon } from './ModelIcon'
import { usePopoverMaxHeight } from './usePopoverMaxHeight'
import { chatTitlebarPillButtonClass } from './platform'
import { listModels, ABU_API_PROVIDER_ID } from '../api/abuApi'
import { ModelAbilityTags } from './ModelAbilityTags'
import { useAbuApiAuth } from '../api/abuApiAuth'
import {
  loadSmartModelEnabled,
  loadSmartModelQuality,
  saveSmartModelEnabled,
  saveSmartModelQuality,
  type SmartModelQuality,
} from './smartModelRouting'
import {
  findModelRoutingRule,
  getModelRoutingPolicy,
  policyModelLabels,
  policyModelDisplayName,
  syncModelRoutingPolicy,
  type ModelRoutingPolicy,
} from './modelRoutingPolicy'

interface ModelSelectorProps {
  currentProviderId: string
  currentModel: string
  onModelChange: (providerId: string, model: string) => void
  /** Opens the in-app ABU account login flow. */
  onOpenLogin?: () => void
}

/** 收藏键：providerId 无冒号，model 可能含冒号 → 只按首个冒号切分。 */
const favKey = (providerId: string, model: string) => `${providerId}:${model}`
const parseFavKey = (key: string): { providerId: string; model: string } | null => {
  const idx = key.indexOf(':')
  if (idx <= 0 || idx >= key.length - 1) return null
  return { providerId: key.slice(0, idx), model: key.slice(idx + 1) }
}

function ModelSelectorBase({
  currentProviderId,
  currentModel,
  onModelChange,
  onOpenLogin,
}: ModelSelectorProps) {
  const t = useT()
  const lang = useLang()
  const { isAuthenticated } = useAbuApiAuth()
  const [open, setOpen] = useState(false)
  const [providers, setProviders] = useState<ModelProvider[]>([])
  const [favorites, setFavorites] = useState<string[]>([])
  const [cloudError, setCloudError] = useState<string | null>(null)
  const [cloudMode, setCloudMode] = useState(false)
  const [smartEnabled, setSmartEnabled] = useState(loadSmartModelEnabled)
  const [smartQuality, setSmartQuality] = useState<SmartModelQuality>(loadSmartModelQuality)
  const [advancedOpen, setAdvancedOpen] = useState(() => !loadSmartModelEnabled())
  const [showAllModels, setShowAllModels] = useState(false)
  const [routingPolicy, setRoutingPolicy] = useState<ModelRoutingPolicy | null>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const maxH = usePopoverMaxHeight(open, menuRef, 'down', 400)

  const loadSettings = useCallback(async () => {
    try {
      const settings = await getSettingsCached()
      const isCloud = settings.runtimeMode?.trim().toLowerCase() === 'cloud'
      setCloudMode(isCloud)

      if (isCloud) {
        // Cloud 模式：从 listModels() 获取模型列表
        setCloudError(null)
        try {
          const response = await listModels()
          // 构造虚拟 Provider（用于 UI 渲染，实际调用时 Rust 侧会替换）
          const virtualProvider: ModelProvider = {
            id: ABU_API_PROVIDER_ID,
            name: 'ABU Cloud',
            apiKeys: [],
            baseUrl: '',
            availableModels: response.models,
            enabledModels: response.models,
            enabled: true,
            apiFormat: 'openai_chat',
          }
          setProviders([virtualProvider])
        } catch (err) {
          console.error('Failed to load cloud models:', err)
          setCloudError(err instanceof Error ? err.message : String(err))
          setProviders([])
        }
      } else {
        // Local 模式：使用本地配置的 providers
        setCloudError(null)
        setProviders(settings.providers || [])
      }

      setFavorites(settings.favoriteModels || [])
    } catch (err) {
      console.error('Failed to load providers:', err)
      setProviders([
        {
          id: currentProviderId || 'dev-provider',
          name: 'Preview',
          apiKeys: [],
          baseUrl: '',
          availableModels: currentModel ? [currentModel] : ['dev-model'],
          enabledModels: currentModel ? [currentModel] : ['dev-model'],
          enabled: true,
          apiFormat: 'openai_chat',
        },
      ])
      setCloudMode(false)
    }
  }, [currentModel, currentProviderId])

  useEffect(() => {
    loadSettings()
    // 设置页自动保存（无保存按钮）与返回聊天视图是并发的：挂载时那次读可能拿到落盘
    // 回包前的旧快照。订阅缓存更新，保存完成后立即拿到新 providers/收藏。
    return subscribeSettings((settings) => {
      // Cloud providers are fetched from the API. A settings-cache broadcast can
      // contain the local provider list and must not overwrite the virtual cloud
      // provider after its model request completes.
      if (settings.runtimeMode?.trim().toLowerCase() === 'cloud') {
        setCloudMode(true)
        // Login/settings refresh can switch an already-mounted chat into Cloud
        // mode; reload the server-backed virtual provider instead of using the
        // local provider array from the settings snapshot.
        void loadSettings()
      } else {
        setCloudMode(false)
        setProviders(settings.providers || [])
        setCloudError(null)
      }
      setFavorites(settings.favoriteModels || [])
    })
  }, [loadSettings])

  useEffect(() => {
    let active = true
    void getModelRoutingPolicy(false).then((policy) => { if (active) setRoutingPolicy(policy) })
    void syncModelRoutingPolicy().then((policy) => { if (active && policy) setRoutingPolicy(policy) })
    return () => { active = false }
  }, [])

  const activeProviders = providers.filter(isProviderEnabled)
  // 只显示有可选模型的服务商，避免没配置模型的服务商变成空的分组标题。
  const visibleProviders = activeProviders
    .map((provider) => ({
      provider,
      models: (provider.enabledModels.length > 0 ? provider.enabledModels : provider.availableModels)
        .filter((model) => showAllModels || !routingPolicy || findModelRoutingRule(routingPolicy, provider.id, model)?.tiers.includes(smartQuality)),
    }))
    .filter((entry) => entry.models.length > 0)

  const currentProvider = activeProviders.find((p) => p.id === currentProviderId)
    ?? providers.find((p) => p.id === currentProviderId)
  const displayName = currentModel || currentProvider?.enabledModels[0] || t.chatSelectModel
  // Tooltip 用模型原始元数据（服务商名 + 完整模型 ID），不截取 UI 上被省略的文本。
  const tooltipText = currentProvider && currentModel
    ? `${currentProvider.name}：${currentModel}`
    : (currentModel || '')
  const preferredRule = routingPolicy?.rules
    .filter((rule) => rule.enabled && rule.healthy && rule.tiers.includes(smartQuality))
    .sort((a, b) => (b.task_scores.creative + b.priority) - (a.task_scores.creative + a.priority))[0]

  // Keep the server-side tier mapping visible so Smart Select does not feel like a black box.
  const cloudTierModels = useMemo(() => {
    const tiers: SmartModelQuality[] = ['fast', 'balanced', 'quality']
    return Object.fromEntries(tiers.map((tier) => {
      const recommended = routingPolicy?.recommended[tier]
      const fallbacks = routingPolicy?.fallbacks[tier] ?? []
      const policyModels = routingPolicy?.rules
        .filter((rule) => rule.enabled && rule.healthy && rule.tiers.includes(tier))
        .sort((a, b) => b.priority - a.priority || b.fallback_priority - a.fallback_priority)
        .map((rule) => rule.model) ?? []
      // The policy is the source of truth for smart routing. Keep its model IDs
      // visible even when the catalog request is briefly behind the policy sync.
      const models = [...new Set([recommended, ...fallbacks, ...policyModels].filter((model): model is string => Boolean(model)))]
      const visibleModels = models.slice(0, 4)
      return [tier, visibleModels]
    })) as Record<SmartModelQuality, string[]>
  }, [routingPolicy])
  const hasCloudTierMap = Boolean(routingPolicy && (
    Object.values(routingPolicy.recommended).some(Boolean)
    || Object.values(routingPolicy.fallbacks).some((models) => Array.isArray(models) && models.length > 0)
    || routingPolicy.rules.some((rule) => rule.tiers.length > 0)
  ))

  // 收藏置顶组：按存储顺序，过滤掉失效的（provider 已删/禁用/模型已不在列表）。
  const favoriteEntries = useMemo(() => {
    return favorites
      .map((key) => {
        const parsed = parseFavKey(key)
        if (!parsed) return null
        const entry = visibleProviders.find((v) => v.provider.id === parsed.providerId)
        if (!entry || !entry.models.includes(parsed.model)) return null
        return { key, providerId: parsed.providerId, providerName: entry.provider.name, model: parsed.model }
      })
      .filter((v): v is { key: string; providerId: string; providerName: string; model: string } => v !== null)
  }, [favorites, visibleProviders])

  const toggleFavorite = useCallback(
    (providerId: string, model: string) => {
      const key = favKey(providerId, model)
      const next = favorites.includes(key)
        ? favorites.filter((k) => k !== key)
        : [...favorites, key]
      const previous = favorites
      setFavorites(next) // 乐观更新
      setFavoriteModelsCached(next).catch((err) => {
        console.error('Failed to save favorite models:', err)
        setFavorites(previous) // 回滚
      })
    },
    [favorites],
  )

  const renderModelRow = (providerId: string, model: string, keySuffix: string) => {
    const selected = !smartEnabled && currentProviderId === providerId && currentModel === model
    const isFav = favorites.includes(favKey(providerId, model))
    const policyRule = findModelRoutingRule(routingPolicy, providerId, model)
    return (
      <div
        key={`${providerId}:${model}:${keySuffix}`}
        className={`group flex w-full items-center gap-1 rounded-lg pr-1 transition-colors ${
          selected
            ? 'bg-neutral-100 dark:bg-neutral-800'
            : 'hover:bg-neutral-50 dark:hover:bg-neutral-800/80'
        }`}
      >
        <button
          type="button"
          onClick={() => {
            setSmartEnabled(false)
            saveSmartModelEnabled(false)
            onModelChange(providerId, model)
            setOpen(false)
          }}
          className={`kv-menu-row min-w-0 flex-1 ${
            selected
              ? 'font-medium text-neutral-900 dark:text-neutral-100'
              : 'text-neutral-700 dark:text-neutral-300'
          }`}
        >
          <ModelIcon model={model} size={16} />
          <span className="min-w-0 truncate">{model}</span>
          {policyRule ? (
            <span className="flex shrink-0 gap-1">
              {policyModelLabels(policyRule, lang).map((label) => <span key={label} className="kv-tag">{label}</span>)}
            </span>
          ) : <ModelAbilityTags model={model} modelOverrides={providers.find((provider) => provider.id === providerId)?.modelOverrides} />}
        </button>
        <button
          type="button"
          aria-label={isFav ? t.chatUnfavorite : t.chatFavorite}
          title={isFav ? t.chatUnfavorite : t.chatFavorite}
          onClick={(e) => {
            e.stopPropagation()
            toggleFavorite(providerId, model)
          }}
          className={`shrink-0 rounded-md p-1.5 transition-colors ${
            isFav
              ? 'text-amber-500'
              : 'text-neutral-300 opacity-0 group-hover:opacity-100 hover:text-amber-500 dark:text-neutral-600'
          }`}
          data-tauri-drag-region="false"
        >
          <Star size={14} fill={isFav ? 'currentColor' : 'none'} />
        </button>
      </div>
    )
  }

  return (
    <div className="relative max-w-full min-w-0" data-tauri-drag-region="false">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        title={smartEnabled ? t.chatSmartModelDescription : (tooltipText || undefined)}
        className={`${chatTitlebarPillButtonClass} max-w-full min-w-0`}
      >
        {smartEnabled ? <Sparkles size={16} className="text-amber-500" /> : currentModel && <ModelIcon model={currentModel} size={16} />}
        <span className="chat-model-selector-label max-w-[200px] truncate font-medium text-neutral-800 dark:text-neutral-200">
          {smartEnabled ? t.chatSmartModel : displayName}
        </span>
        <ChevronDown
          size={15}
          className={`chat-model-selector-caret shrink-0 text-neutral-400 transition-transform duration-[var(--kv-dur-fast)] ease-[var(--kv-ease-standard)] ${open ? 'rotate-180' : ''}`}
        />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} aria-hidden />
          <div ref={menuRef} style={{ maxHeight: maxH }} className="chat-model-selector-menu chat-motion-popover absolute left-0 top-full z-20 mt-2 min-w-[240px] overflow-y-auto kv-menu">
            <div className="px-1 py-1">
              <button
                type="button"
                onClick={() => {
                  setSmartEnabled(true)
                  saveSmartModelEnabled(true)
                }}
                className={`flex w-full items-start gap-2.5 rounded-lg px-3 py-2.5 text-left transition-colors ${
                  smartEnabled
                    ? 'bg-amber-50 text-neutral-900 dark:bg-amber-400/10 dark:text-neutral-100'
                    : 'text-neutral-700 hover:bg-neutral-50 dark:text-neutral-300 dark:hover:bg-neutral-800/80'
                }`}
              >
                <Sparkles size={17} className="mt-0.5 shrink-0 text-amber-500" />
                <span className="min-w-0 flex-1">
                  <span className="flex items-center justify-between gap-3 text-[13px] font-semibold">
                    {t.chatSmartModel}
                    {smartEnabled && <Check size={15} className="text-amber-500" />}
                  </span>
                  <span className="mt-0.5 block text-[11px] leading-4 text-neutral-500 dark:text-neutral-400">
                    {t.chatSmartModelDescription}
                  </span>
                  {preferredRule && (
                    <span className="mt-1 block text-[11px] leading-4 text-amber-700 dark:text-amber-300">
                      {t.chatSmartModelPreferenceResult
                        .replace('{tier}', smartQuality === 'fast' ? t.chatSmartModelFast : smartQuality === 'balanced' ? t.chatSmartModelBalanced : t.chatSmartModelQuality)
                        .replace('{model}', policyModelDisplayName(preferredRule.model))}
                    </span>
                  )}
                </span>
              </button>
              <div className="px-3 pb-2 pt-2">
                <div className="mb-1.5 text-[10px] font-medium text-neutral-400">{t.chatSmartModelPreference}</div>
                <div className="grid grid-cols-3 gap-1 rounded-lg bg-neutral-100 p-1 dark:bg-neutral-800">
                  {(['fast', 'balanced', 'quality'] as const).map((quality) => (
                    <button
                      key={quality}
                      type="button"
                      onClick={() => {
                        setSmartEnabled(true)
                        setSmartQuality(quality)
                        saveSmartModelEnabled(true)
                        saveSmartModelQuality(quality)
                      }}
                      className={`rounded-md px-2 py-1.5 text-[11px] font-medium transition-colors ${
                        smartQuality === quality
                          ? 'bg-white text-neutral-900 shadow-sm dark:bg-neutral-700 dark:text-neutral-100'
                          : 'text-neutral-500 hover:text-neutral-800 dark:text-neutral-400 dark:hover:text-neutral-200'
                      }`}
                    >
                      {quality === 'fast'
                        ? t.chatSmartModelFast
                        : quality === 'balanced'
                          ? t.chatSmartModelBalanced
                          : t.chatSmartModelQuality}
                    </button>
                  ))}
                </div>
                {cloudMode && hasCloudTierMap && (
                  <div className="mt-2 grid grid-cols-3 gap-1.5" aria-label={lang === 'zh' ? '云端档位模型' : 'Cloud tier models'}>
                    {(['fast', 'balanced', 'quality'] as const).map((tier) => {
                      const models = cloudTierModels[tier]
                      const label = tier === 'fast'
                        ? t.chatSmartModelFast
                        : tier === 'balanced'
                          ? t.chatSmartModelBalanced
                          : t.chatSmartModelQuality
                      return (
                        <div
                          key={tier}
                          className={`min-w-0 rounded-md border px-2 py-1.5 ${smartQuality === tier ? 'border-amber-200 bg-amber-50/70 dark:border-amber-500/30 dark:bg-amber-400/10' : 'border-neutral-100 bg-white/70 dark:border-neutral-700 dark:bg-neutral-900/40'}`}
                          title={models.join(', ') || (lang === 'zh' ? '暂无可用模型' : 'No models available')}
                        >
                          <div className="text-[10px] font-medium text-neutral-400">{label}{lang === 'zh' ? '档位' : ' tier'}</div>
                          <div className="mt-0.5 truncate text-[10px] text-neutral-700 dark:text-neutral-300">
                            {models.length > 0
                              ? `${lang === 'zh' ? '模型：' : 'Model: '}${models[0]}`
                              : (lang === 'zh' ? '暂无' : 'None')}
                          </div>
                          {models.length > 1 && <div className="text-[9px] text-neutral-400">+{models.length - 1}</div>}
                        </div>
                      )
                    })}
                  </div>
                )}
              </div>
              <button
                type="button"
                onClick={() => setAdvancedOpen((value) => !value)}
                className="flex w-full items-center justify-center gap-1 border-t border-neutral-100 px-3 py-2 text-[11px] text-neutral-500 hover:text-neutral-800 dark:border-neutral-800 dark:text-neutral-400 dark:hover:text-neutral-200"
                aria-expanded={advancedOpen}
              >
                {t.chatSmartModelAdvanced}
                <ChevronDown size={12} className={`transition-transform ${advancedOpen ? 'rotate-180' : ''}`} />
              </button>
            </div>
            {advancedOpen && (
              <div className="border-t border-neutral-100 pt-1 dark:border-neutral-800">
                <label className="flex items-center justify-between gap-3 px-4 py-2 text-[11px] text-neutral-600 dark:text-neutral-400">
                  <span>{t.chatSmartModelShowAll}</span>
                  <input type="checkbox" checked={showAllModels} onChange={(event) => setShowAllModels(event.target.checked)} />
                </label>
            {favoriteEntries.length > 0 && (
              <div className="px-1 py-1">
                <div className="flex items-center gap-1 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wide text-amber-500">
                  <Star size={11} fill="currentColor" />
                  {t.chatFavorites}
                </div>
                {favoriteEntries.map((entry) =>
                  renderModelRow(entry.providerId, entry.model, 'fav'),
                )}
              </div>
            )}
            {visibleProviders.map(({ provider, models }) => (
              <div key={provider.id} className="px-1 py-1">
                <div className="px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wide text-neutral-400">
                  {provider.name}
                </div>
                {models.map((model) => renderModelRow(provider.id, model, 'grp'))}
              </div>
            ))}
            {visibleProviders.length === 0 && (
              <div className="px-4 py-6 text-center text-sm text-neutral-500">
                {!isAuthenticated ? (
                  <>
                    <div className="font-medium text-neutral-700 dark:text-neutral-300">{t.chatNoModels}</div>
                    <p className="mt-2 text-xs text-neutral-600 dark:text-neutral-400">
                      {t.onboardingLoginDesc}
                    </p>
                    <button
                      type="button"
                      onClick={() => {
                        onOpenLogin?.()
                        if (!onOpenLogin) window.location.hash = '#chat/onboarding?step=login&return=chat'
                        setOpen(false)
                      }}
                      className="mt-4 rounded-md bg-indigo-500 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-600"
                    >
                      {t.onboardingStepLogin}
                    </button>
                    <p className="mt-3 text-xs text-neutral-500">
                      登录后仍可在设置中添加模型提供商
                    </p>
                  </>
                ) : cloudError ? (
                  <>
                    <div className="font-medium text-neutral-700 dark:text-neutral-300">{t.chatNoModels}</div>
                    <div className="mt-2 text-xs text-red-500">{cloudError}</div>
                    <div className="mt-3 space-y-2 text-xs text-neutral-600 dark:text-neutral-400">
                      <p>{'请检查：'}</p>
                      <ul className="list-inside list-disc space-y-1 text-left">
                        <li>{'1. 是否已完成 ABU API 账户登录'}</li>
                        <li>{'2. 网络连接是否正常'}</li>
                        <li>{'3. API 配置是否正确（设置 → 查家）'}</li>
                      </ul>
                    </div>
                    <button
                      type="button"
                      onClick={() => void loadSettings()}
                      className="mt-4 rounded-md bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-700 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
                    >
                      {t.onboardingRetry || '重试'}
                    </button>
                  </>
                ) : (
                  <>
                    <div className="font-medium text-neutral-700 dark:text-neutral-300">{t.chatNoModels}</div>
                    <div className="mt-3 space-y-2 text-xs text-neutral-600 dark:text-neutral-400">
                      <p>{'请先配置模型提供商：'}</p>
                      <p className="text-indigo-500">{'设置 → 模型 → 添加提供商'}</p>
                    </div>
                  </>
                )}
              </div>
            )}
              </div>
            )}
          </div>
        </>
      )}
    </div>
  )
}

// memo：顶栏选择器，仅在 props 变化时重渲，避免 Chat 重渲时跟着白渲。
export const ModelSelector = memo(ModelSelectorBase)
