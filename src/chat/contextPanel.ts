import type { I18n } from '../settings/i18n'
import type { ContextUsageSegment } from './types'

/** 与 `commands.rs` 中 `AUTO_COMPRESS_RATIO` 保持一致 */
export const CONTEXT_AUTO_COMPRESS_PERCENT = 85
export const CONTEXT_WARNING_PERCENT = 70
export const CONTEXT_CRITICAL_PERCENT = 95

export const CONTEXT_FREE_SEGMENT_ID = '__free__'
export const CONTEXT_FREE_COLOR = '#E8E8ED'

export function segmentTokens(segment: ContextUsageSegment): number {
  return segment.estimated_tokens ?? segment.estimatedTokens ?? 0
}

const SEGMENT_LABEL_KEY: Record<string, keyof I18n> = {
  system_prompt: 'contextSegmentSystemPrompt',
  assistant: 'contextSegmentAssistant',
  set: 'contextSegmentSet',
  runtime_context: 'contextSegmentRuntime',
  memory_l1: 'contextSegmentMemory',
  knowledge_base: 'contextSegmentKnowledgeBase',
  agent_plan: 'contextSegmentAgentPlan',
  agent_todo: 'contextSegmentAgentTodo',
  tool_definitions: 'contextSegmentToolDefinitions',
  native_tools: 'contextSegmentNativeTools',
  skills: 'contextSegmentSkills',
  mcp: 'contextSegmentMcp',
  summarized_conversation: 'contextSegmentSummarized',
  conversation: 'contextSegmentConversation',
  attachments: 'contextSegmentAttachments',
}

export function localizedSegmentLabel(segment: ContextUsageSegment, t: I18n): string {
  const key = SEGMENT_LABEL_KEY[segment.id]
  if (key && t[key]) return String(t[key])
  return segment.label
}

export type ContextBarSlice = {
  id: string
  label: string
  tokens: number
  color: string
  widthPercent: number
}

export function buildContextBarSlices(
  segments: ContextUsageSegment[],
  estimatedInputTokens: number,
  contextWindowTokens: number | null,
  t: I18n,
): ContextBarSlice[] {
  const active = segments.filter((segment) => segmentTokens(segment) > 0)
  const window = contextWindowTokens ?? 0
  const denominator = window > 0 ? window : Math.max(estimatedInputTokens, 1)

  const slices: ContextBarSlice[] = active.map((segment) => {
    const tokens = segmentTokens(segment)
    return {
      id: segment.id,
      label: localizedSegmentLabel(segment, t),
      tokens,
      color: segment.color || '#7A7A7A',
      widthPercent: Math.max(0, (tokens / denominator) * 100),
    }
  })

  if (window > 0) {
    const freeTokens = Math.max(0, window - estimatedInputTokens)
    if (freeTokens > 0) {
      slices.push({
        id: CONTEXT_FREE_SEGMENT_ID,
        label: t.contextSegmentFree,
        tokens: freeTokens,
        color: CONTEXT_FREE_COLOR,
        widthPercent: (freeTokens / window) * 100,
      })
    }
  }

  return slices
}
