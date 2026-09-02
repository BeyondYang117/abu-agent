import { resolveModelInfo } from '../data/modelMatching'
import { useT } from '../settings/i18n'
import type { ModelInfo } from '../api/tauri'
import { getModelAbilityTags } from './modelCapabilities'

interface ModelAbilityTagsProps {
  model: string
  modelOverrides?: Record<string, ModelInfo>
}

const tagStyles: Record<string, string> = {
  reasoning: 'bg-violet-500/10 text-violet-600 dark:text-violet-300',
  vision: 'bg-sky-500/10 text-sky-600 dark:text-sky-300',
  image: 'bg-fuchsia-500/10 text-fuchsia-600 dark:text-fuchsia-300',
  embedding: 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-300',
  code: 'bg-amber-500/10 text-amber-700 dark:text-amber-300',
  general: 'bg-neutral-500/10 text-neutral-500 dark:text-neutral-400',
}

export function ModelAbilityTags({ model, modelOverrides }: ModelAbilityTagsProps) {
  const t = useT()
  const tags = getModelAbilityTags(model, resolveModelInfo(model, modelOverrides))
  const labels = {
    reasoning: t.chatModelTagReasoning,
    vision: t.chatModelTagVision,
    image: t.chatModelTagImage,
    embedding: t.chatModelTagEmbedding,
    code: t.chatModelTagCode,
    general: t.chatModelTagGeneral,
  } as const

  return (
    <span className="ml-auto flex shrink-0 items-center gap-1" aria-label={tags.map((tag) => labels[tag]).join(', ')}>
      {tags.map((tag) => (
        <span
          key={tag}
          className={`rounded px-1 py-0.5 text-[10px] font-medium leading-none ${tagStyles[tag]}`}
        >
          {labels[tag]}
        </span>
      ))}
    </span>
  )
}
