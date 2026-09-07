/** 支持网页登录授权的 ABU 平台域名池。顺序即故障转移优先级。 */
/** 充值、订阅等平台页面固定使用主站，不能跟随 API 故障转移域名。 */
export const ABU_PLATFORM_URL = 'https://api.abuai.chat'

export const ABU_API_BASE_URLS = [
  ABU_PLATFORM_URL,
  'https://api.abusz.com',
] as const

const API_ONLY_BASE_URLS = new Set(['https://api.abu117.cn'])

export type AbuApiBaseUrl = (typeof ABU_API_BASE_URLS)[number]

export function normalizeAbuApiBaseUrl(value: string | null | undefined): string {
  const trimmed = (value || '').trim().replace(/\/+$/, '')
  return !trimmed || API_ONLY_BASE_URLS.has(trimmed) ? ABU_API_BASE_URLS[0] : trimmed
}

export function getAbuApiEndpointCandidates(current?: string | null): string[] {
  const normalized = normalizeAbuApiBaseUrl(current)
  return [normalized, ...ABU_API_BASE_URLS.filter((url) => url !== normalized)]
}

/** Execute an API operation against each configured edge until one succeeds. */
export async function requestWithAbuApiEndpointFailover<T>(
  current: string | null | undefined,
  request: (baseUrl: string) => Promise<T>,
): Promise<{ baseUrl: string; value: T }> {
  let firstError: unknown
  for (const baseUrl of getAbuApiEndpointCandidates(current)) {
    try {
      return { baseUrl, value: await request(baseUrl) }
    } catch (error) {
      firstError ??= error
    }
  }
  throw firstError instanceof Error ? firstError : new Error(String(firstError || 'API request failed'))
}

/** 轻量健康探测，不要求登录；服务端应返回 2xx/401/403 即视为可达。 */
export async function probeAbuApiEndpoint(baseUrl: string, timeoutMs = 5000): Promise<boolean> {
  const controller = new AbortController()
  const timer = window.setTimeout(() => controller.abort(), timeoutMs)
  try {
    const response = await fetch(`${normalizeAbuApiBaseUrl(baseUrl)}/api/agent/models`, {
      method: 'GET',
      signal: controller.signal,
    })
    return response.ok || response.status === 401 || response.status === 403
  } catch {
    return false
  } finally {
    window.clearTimeout(timer)
  }
}

export async function selectHealthyAbuApiEndpoint(current?: string | null): Promise<string> {
  const candidates = getAbuApiEndpointCandidates(current)
  const results = await Promise.all(candidates.map(async (url) => ({ url, ok: await probeAbuApiEndpoint(url) })))
  return results.find((result) => result.ok)?.url || candidates[0]
}
