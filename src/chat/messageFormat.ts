/** 与参考 UI 一致：Jun 1, 2026 at 12:30 AM */
export function formatAssistantMessageTime(timestamp: number): string {
  const date = new Date(timestamp * 1000)
  const datePart = new Intl.DateTimeFormat('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  }).format(date)
  const timePart = new Intl.DateTimeFormat('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    hour12: true,
  }).format(date)
  return `${datePart} at ${timePart}`
}
