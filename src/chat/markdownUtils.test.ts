import { describe, expect, it } from 'vitest'
import { normalizeMarkdownForRender } from './markdownUtils'

describe('normalizeMarkdownForRender', () => {
  it('inserts row breaks between inline GFM table rows', () => {
    const input = '| a | b | | c | d |'
    expect(normalizeMarkdownForRender(input)).toBe('| a | b |\n| c | d |')
  })

  it('leaves already multiline tables unchanged', () => {
    const input = '| a | b |\n| c | d |'
    expect(normalizeMarkdownForRender(input)).toBe(input)
  })
})
