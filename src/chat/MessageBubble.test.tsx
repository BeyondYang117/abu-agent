import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { MessageBubble } from './MessageBubble'
import type { ChatMessage } from './types'

describe('MessageBubble timeline orphan tools', () => {
  it('renders tool calls that are missing tool segments', () => {
    const message: ChatMessage = {
      id: 'msg-1',
      role: 'assistant',
      content: 'done',
      reasoning: 'thinking',
      segments: [
        {
          id: 'seg-reasoning',
          kind: 'reasoning',
          phase: 'plain',
          order: 1,
          text: 'thinking',
        },
        {
          id: 'seg-text',
          kind: 'text',
          phase: 'plain',
          order: 2,
          text: 'done',
        },
      ],
      tool_calls: [
        {
          id: 'tool-1',
          name: 'Read',
          source: 'external_cli',
          status: 'success',
          arguments: '{"path":"README.md"}',
        },
      ],
      timestamp: 1,
    }

    render(<MessageBubble message={message} />)
    expect(screen.getByText('Read')).toBeInTheDocument()
  })
})
