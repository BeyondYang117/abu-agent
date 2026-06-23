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

describe('MessageBubble timeline grouping', () => {
  it('collapses a completed group into a one-line summary by default', () => {
    const message: ChatMessage = {
      id: 'msg-2',
      role: 'assistant',
      content: 'answer',
      segments: [
        { id: 'seg-r', kind: 'reasoning', phase: 'plain', order: 1, text: 'planning' },
        { id: 'seg-t', kind: 'tool', phase: 'tool_loop', order: 2, tool_call_id: 'tool-1' },
        { id: 'seg-text', kind: 'text', phase: 'plain', order: 3, text: 'answer' },
      ],
      tool_calls: [
        {
          id: 'tool-1',
          name: 'read_file',
          source: 'native',
          status: 'completed',
          arguments: '{"path":"a.ts"}',
        },
      ],
      timestamp: 1,
    }

    render(<MessageBubble message={message} />)
    expect(screen.getByText(/读取文件 · 1 步 · 已完成/)).toBeInTheDocument()
    // collapsed group hides the reasoning body region
    expect(screen.getByLabelText('过程分组')).toHaveAttribute('aria-label', '过程分组')
    // final answer text still renders
    expect(screen.getByText('answer')).toBeInTheDocument()
  })

  it('renders tool → text → tool as two separate groups', () => {
    const message: ChatMessage = {
      id: 'msg-3',
      role: 'assistant',
      content: 'final',
      segments: [
        { id: 'g1', kind: 'tool', phase: 'tool_loop', order: 1, tool_call_id: 'c1' },
        { id: 'txt', kind: 'text', phase: 'plain', order: 2, text: 'middle' },
        { id: 'g2', kind: 'tool', phase: 'tool_loop', order: 3, tool_call_id: 'c2' },
      ],
      tool_calls: [
        { id: 'c1', name: 'run_command', source: 'native', status: 'completed' },
        { id: 'c2', name: 'web_fetch', source: 'native', status: 'completed' },
      ],
      timestamp: 1,
    }

    render(<MessageBubble message={message} />)
    expect(screen.getAllByLabelText('过程分组')).toHaveLength(2)
    expect(screen.getByText('middle')).toBeInTheDocument()
  })

  it('keeps a generating group expanded with a running summary', () => {
    const message: ChatMessage = {
      id: 'msg-4',
      role: 'assistant',
      content: '',
      segments: [
        { id: 'seg-t', kind: 'tool', phase: 'tool_loop', order: 1, tool_call_id: 'tool-1' },
      ],
      tool_calls: [
        {
          id: 'tool-1',
          name: 'run_command',
          source: 'native',
          status: 'running',
        },
      ],
      timestamp: 1,
    }

    render(<MessageBubble message={message} />)
    expect(screen.getByText(/执行命令 · 1 步 · 进行中…/)).toBeInTheDocument()
    // running tool block detail still renders inside the expanded group
    expect(screen.getByText('run_command')).toBeInTheDocument()
  })
})
