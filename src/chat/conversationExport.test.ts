import { describe, expect, it } from 'vitest'
import { conversationMarkdownFilename } from './conversationExport'

describe('conversationMarkdownFilename', () => {
  it('keeps a readable title and adds the Markdown extension', () => {
    expect(conversationMarkdownFilename('GitHub 官网')).toBe('GitHub 官网.md')
  })

  it('replaces cross-platform invalid characters and trailing dots', () => {
    expect(conversationMarkdownFilename('a/b:c*? "test"...')).toBe('a b c test.md')
  })

  it('falls back for empty and protects Windows reserved names', () => {
    expect(conversationMarkdownFilename('  ...  ')).toBe('conversation.md')
    expect(conversationMarkdownFilename('CON')).toBe('conversation-CON.md')
  })

  it('caps long titles by Unicode characters', () => {
    const filename = conversationMarkdownFilename('对'.repeat(100))
    expect([...filename.slice(0, -3)]).toHaveLength(80)
  })
})
