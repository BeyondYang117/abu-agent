/** 估算 token 数：ASCII 按 ~4 字符/token；非 ASCII（中日韩等）按 1 字符/token */
export function estimateTokens(text: string): number {
  let ascii = 0
  for (let i = 0; i < text.length; i++) {
    if (text.charCodeAt(i) < 128) ascii++
  }
  const nonAscii = text.length - ascii
  return Math.ceil(ascii / 4 + nonAscii)
}

export function formatTokens(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`
  return `${n}`
}
