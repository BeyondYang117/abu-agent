import type { ComponentType, CSSProperties } from 'react'
// Import the exact Color/Mono leaf component FILE (not the brand index, which also
// pulls .Avatar -> features/IconAvatar -> @lobehub/ui -> antd6/React19). This keeps
// us React-18-clean and guarantees antd never enters the bundle. See task research.
import OpenAI from '@lobehub/icons/es/OpenAI/components/Mono'
import Grok from '@lobehub/icons/es/Grok/components/Mono'
import Moonshot from '@lobehub/icons/es/Moonshot/components/Mono'
import Claude from '@lobehub/icons/es/Claude/components/Color'
import Gemini from '@lobehub/icons/es/Gemini/components/Color'
import Gemma from '@lobehub/icons/es/Gemma/components/Color'
import DeepSeek from '@lobehub/icons/es/DeepSeek/components/Color'
import Qwen from '@lobehub/icons/es/Qwen/components/Color'
import ChatGLM from '@lobehub/icons/es/ChatGLM/components/Color'
import Mistral from '@lobehub/icons/es/Mistral/components/Color'
import Meta from '@lobehub/icons/es/Meta/components/Color'
import Yi from '@lobehub/icons/es/Yi/components/Color'
import Doubao from '@lobehub/icons/es/Doubao/components/Color'
import Wenxin from '@lobehub/icons/es/Wenxin/components/Color'
import Minimax from '@lobehub/icons/es/Minimax/components/Color'
import Cohere from '@lobehub/icons/es/Cohere/components/Color'
import Microsoft from '@lobehub/icons/es/Microsoft/components/Color'
import Stepfun from '@lobehub/icons/es/Stepfun/components/Color'

// lobehub leaf icons declare `size?: string | number`; widen via a loose cast so the
// map stays typed without fighting their prop types.
type Glyph = ComponentType<{ size?: number; style?: CSSProperties }>
const G = (icon: unknown) => icon as Glyph

// First match wins; tested case-insensitively against the model id.
const MODEL_ICON_MAP: Array<[RegExp, Glyph]> = [
  [/gpt|chatgpt|openai|codex|dall[-·]?e|(?:^|[-/])o[134](?:-|$)/, G(OpenAI)],
  [/claude|anthropic/, G(Claude)],
  [/gemma/, G(Gemma)],
  [/gemini|palm|bison/, G(Gemini)],
  [/deepseek/, G(DeepSeek)],
  [/qwen|qwq|qvq|tongyi|wanx/, G(Qwen)],
  [/grok/, G(Grok)],
  [/kimi|moonshot/, G(Moonshot)],
  [/glm|chatglm|zhipu/, G(ChatGLM)],
  [/mistral|mixtral|codestral|pixtral|ministral|magistral|devstral/, G(Mistral)],
  [/llama|llava/, G(Meta)],
  [/(?:^|[-/])yi-/, G(Yi)],
  [/doubao/, G(Doubao)],
  [/ernie|wenxin/, G(Wenxin)],
  [/minimax|abab/, G(Minimax)],
  [/cohere|command/, G(Cohere)],
  [/(?:^|[-/])phi-|wizardlm/, G(Microsoft)],
  [/(?:^|[-/])step-/, G(Stepfun)],
]

function matchGlyph(model: string): Glyph | null {
  const id = model.toLowerCase()
  for (const [re, glyph] of MODEL_ICON_MAP) {
    if (re.test(id)) return glyph
  }
  return null
}

interface ModelIconProps {
  model: string
  size?: number
  className?: string
}

export function ModelIcon({ model, size = 18, className }: ModelIconProps) {
  const Brand = matchGlyph(model)
  if (Brand) {
    return (
      <span className={className} style={{ display: 'inline-flex', flexShrink: 0 }} aria-hidden="true">
        <Brand size={size} />
      </span>
    )
  }
  // Fallback placeholder — mirrors AgentIcon's initial chip.
  return (
    <span
      className={`inline-flex shrink-0 items-center justify-center rounded-md bg-neutral-200 text-[9px] font-semibold uppercase text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300 ${className ?? ''}`}
      style={{ width: size, height: size }}
      aria-hidden="true"
    >
      {model.replace(/[^a-z0-9]/gi, '').slice(0, 2) || '?'}
    </span>
  )
}

// eslint-disable-next-line react-refresh/only-export-components -- test-only helper
export { matchGlyph as _matchGlyphForTest }
