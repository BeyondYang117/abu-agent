/**
 * Relay model catalogs are platform-wide. Keep the input editable, but make
 * suggestions relevant to the CLI being configured.
 */
const NON_CHAT_MODEL = /(?:embedding|embed|rerank|moderation|moderations|whisper|tts|speech|audio|image|vision-only|dall-e|sora|video|realtime|search-preview)/i

const CLI_PATTERNS: Record<string, RegExp> = {
  // Codex app-server currently recommends only the Codex family and the
  // GPT-5.5/5.6 coding models. Generic GPT-4o/5, mini/nano, chat snapshots,
  // and the o1/o3/o4 families are deliberately left out of suggestions.
  codex: /^(?:(?:openai|azure)[/:])?(?:codex(?:[-_./].*)?|gpt-5\.(?:5|6)(?:[-_./].*)?)/i,
  claude: /^(?:(?:anthropic)[/:])?(?:claude(?:[-_./].*)?|anthropic(?:[-_./].*)?)/i,
  gemini: /^(?:(?:google)[/:])?(?:gemini(?:[-_./].*)?|gemma(?:[-_./].*)?)/i,
  kimi: /^(?:(?:moonshot)[/:])?(?:kimi(?:[-_./].*)?|moonshot(?:[-_./].*)?)/i,
  grok: /^(?:(?:xai)[/:])?(?:grok(?:[-_./].*)?|xai(?:[-_./].*)?)/i,
  dsh: /^(?:(?:deepseek)[/:])?deepseek(?:[-_./].*)?/i,
}

const KNOWN_MODEL_FAMILY = /^(?:(?:openai|azure|anthropic|google|moonshot|xai|deepseek)[/:])?(?:claude|anthropic|gpt|codex|o[1-9]|gemini|gemma|kimi|moonshot|grok|xai|deepseek|qwen|llama|mistral|minimax|glm|yi)(?:[-_./]|$)/i

const MAX_SUGGESTIONS = 24

export function filterCliModels(agentId: string, models: readonly string[]): string[] {
  const unique: string[] = []
  const seen = new Set<string>()
  for (const raw of models) {
    const model = raw.trim()
    if (!model || seen.has(model) || NON_CHAT_MODEL.test(model)) continue
    seen.add(model)
    unique.push(model)
  }
  const pattern = CLI_PATTERNS[agentId]
  if (!pattern) return unique.slice(0, MAX_SUGGESTIONS)
  // Prefer the CLI family. If a provider uses custom aliases, keep only
  // aliases whose brand cannot be identified; never leak Claude models into
  // Codex (or vice versa) just to make the dropdown non-empty.
  const family = unique.filter((model) => pattern.test(model))
  const custom = unique.filter((model) => !KNOWN_MODEL_FAMILY.test(model))
  return (family.length > 0 ? family : custom).slice(0, MAX_SUGGESTIONS)
}
