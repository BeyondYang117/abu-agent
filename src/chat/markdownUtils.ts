/**
 * Models sometimes emit GFM tables on one line (`| a | b | | c | d |`) when asked
 * to avoid blank lines. Restore row breaks so remark-gfm can parse tables.
 */
export function normalizeMarkdownForRender(content: string): string {
  return content.replace(/(\|(?:[^|\n]+\|){2,})\s*(\|)/g, '$1\n$2')
}
