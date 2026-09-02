/** ABU API quota units: 500,000 units represent one USD by default. */
export const ABU_QUOTA_PER_USD = 500_000

export function formatAbuQuota(quota: number): string {
  return (quota / ABU_QUOTA_PER_USD).toFixed(2)
}
