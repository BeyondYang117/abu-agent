export function resolveAccountDisplayName(
  account: { username?: string; displayName?: string } | null,
  fallback: string,
): string {
  const username = account?.username?.trim().replace(/^@+/, '')
  return username || account?.displayName?.trim() || fallback
}
