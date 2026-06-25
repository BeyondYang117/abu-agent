// 知识库「检索」设置：hybrid(向量+关键词 RRF) 开关与权重 + 可选全局 rerank。
// 只配 embedding 即可用；hybrid 免配可关，rerank 留空即关、失败自动降级。
import { Info } from 'lucide-react'
import { type ModelProvider, type KnowledgeBaseConfig } from '../api/tauri'
import { type Lang } from './i18n'
import { SettingsGroup, Input, Select, Toggle } from './components'

const DEFAULT: KnowledgeBaseConfig = {
  hybridEnabled: true,
  weightVector: 1,
  weightKeyword: 1,
  rerankProviderId: '',
  rerankModel: '',
}

export function RetrievalPanel({
  config,
  providers,
  lang,
  onChange,
}: {
  config?: KnowledgeBaseConfig
  providers: ModelProvider[]
  lang: Lang
  onChange: (next: KnowledgeBaseConfig) => void
}) {
  const t = (zh: string, en: string) => (lang === 'zh' ? zh : en)
  const cfg = config ?? DEFAULT
  const patch = (u: Partial<KnowledgeBaseConfig>) => onChange({ ...cfg, ...u })

  const enabled = providers.filter((p) => p.enabled !== false)
  const rerankProvider = enabled.find((p) => p.id === cfg.rerankProviderId)
  const rerankModels = rerankProvider?.enabledModels ?? []

  return (
    <SettingsGroup title={t('检索', 'Retrieval')}>
      <p className="flex items-start gap-1.5 px-1 py-1 text-xs text-zinc-500">
        <Info size={13} className="mt-0.5 shrink-0" />
        <span>
          {t(
            '只配一个 embedding 模型即可检索。Hybrid 会在向量召回外加关键词(BM25)并用 RRF 融合（关键词对中文短词/精确匹配有用）。Rerank 可选：配了就在召回后重排，留空或失败则按原顺序。',
            'A single embedding model is enough to search. Hybrid adds keyword (BM25) recall fused with vectors via RRF (helps CJK short terms / exact match). Rerank is optional: when set it reorders results, otherwise the fused order is used.',
          )}
        </span>
      </p>

      <label className="flex cursor-pointer items-center gap-2 px-1 py-1.5 text-sm text-zinc-700 dark:text-zinc-200">
        <Toggle checked={cfg.hybridEnabled} onChange={(v) => patch({ hybridEnabled: v })} />
        {t('启用 Hybrid（向量 + 关键词 RRF 融合）', 'Enable hybrid (vector + keyword RRF)')}
      </label>

      {cfg.hybridEnabled && (
        <div className="flex flex-wrap items-center gap-4 px-1 py-1 text-sm">
          <label className="flex items-center gap-2">
            <span className="text-zinc-500">{t('向量权重', 'Vector weight')}</span>
            <Input
              type="number"
              className="w-20"
              value={String(cfg.weightVector)}
              onChange={(v) => patch({ weightVector: Number(v) || 0 })}
            />
          </label>
          <label className="flex items-center gap-2">
            <span className="text-zinc-500">{t('关键词权重', 'Keyword weight')}</span>
            <Input
              type="number"
              className="w-20"
              value={String(cfg.weightKeyword)}
              onChange={(v) => patch({ weightKeyword: Number(v) || 0 })}
            />
          </label>
          <span className="text-xs text-zinc-400">
            {t('强 embedding 模型下纯向量可能已足够，可按实际效果调。', 'Pure vector may suffice with a strong embedding model — tune to taste.')}
          </span>
        </div>
      )}

      <div className="mt-2 flex flex-wrap items-center gap-2 px-1 py-1 text-sm">
        <span className="text-zinc-500">{t('Rerank（重排）', 'Rerank')}</span>
        <Select
          className="w-44"
          value={cfg.rerankProviderId}
          onChange={(pid) => patch({ rerankProviderId: pid, rerankModel: '' })}
          options={[
            { value: '', label: t('关闭', 'Off') },
            ...enabled.map((p) => ({ value: p.id, label: p.name || p.id })),
          ]}
        />
        {cfg.rerankProviderId && (
          <Select
            className="w-56"
            value={cfg.rerankModel}
            onChange={(m) => patch({ rerankModel: m })}
            options={[
              { value: '', label: t('选择 rerank 模型…', 'Pick rerank model…') },
              ...rerankModels.map((m) => ({ value: m, label: m })),
            ]}
          />
        )}
        <span className="text-xs text-zinc-400">
          {t('Cohere / Jina 兼容；留空即关闭，调用失败自动降级。', 'Cohere/Jina-compatible; blank = off, failures degrade gracefully.')}
        </span>
      </div>
    </SettingsGroup>
  )
}
