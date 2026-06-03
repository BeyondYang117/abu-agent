/** Human-friendly preview for tool stdout (e.g. Tavily JSON) in the tool call UI. */

function compact(text: string, max: number): string {
  const cleaned = text.replace(/\s+/g, ' ').trim()
  if (cleaned.length <= max) return cleaned
  return `${cleaned.slice(0, max).trimEnd()}…`
}

function jsonBody(text: string): string | null {
  const trimmed = text.trim()
  const body = trimmed.startsWith('stdout:') ? trimmed.slice('stdout:'.length).trim() : trimmed
  if (!body.startsWith('{') && !body.startsWith('[')) return null
  return body
}

export function formatToolResultPreview(raw: string, max = 220): string {
  if (!raw.trim()) return ''
  const body = jsonBody(raw)
  if (!body) return compact(raw, max)

  try {
    const parsed = JSON.parse(body) as Record<string, unknown>
    const answer = typeof parsed.answer === 'string' ? parsed.answer.trim() : ''
    if (answer) return compact(`答: ${answer}`, max)

    const results = Array.isArray(parsed.results) ? parsed.results : []
    const query = typeof parsed.query === 'string' ? parsed.query.trim() : ''
    const queryLabel = query ? `「${query}」` : ''

    if (results.length > 0) {
      const first = results[0] as Record<string, unknown>
      const title =
        (typeof first.title === 'string' && first.title) ||
        (typeof first.url === 'string' && first.url) ||
        ''
      const snippet =
        typeof first.content === 'string'
          ? first.content.replace(/\s+/g, ' ').trim()
          : typeof first.raw_content === 'string'
            ? first.raw_content.replace(/\s+/g, ' ').trim()
            : ''
      const head = `${results.length} 条结果${queryLabel}`
      const detail = [title, snippet].filter(Boolean).join(' — ')
      return compact(detail ? `${head}: ${detail}` : head, max)
    }

    if (Array.isArray(parsed.failed_results) && parsed.failed_results.length === 0) {
      return compact('页面提取完成（无失败项）', max)
    }
  } catch {
    // fall through
  }

  return compact(raw, max)
}
