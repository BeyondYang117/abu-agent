import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { MessageBubble } from './MessageBubble'
import { MessageList } from './MessageList'
import type { ChatMessage } from './types'

function assistantMessage(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id: 'assistant-1',
    role: 'assistant',
    content: '',
    timestamp: 1,
    segments: [
      {
        id: 'reasoning-1',
        kind: 'reasoning',
        phase: 'plain',
        order: 0,
        text: 'first thought',
      },
    ],
    ...overrides,
  }
}

describe('MessageBubble reasoning durations', () => {
  it('renders non-image artifacts as generated file cards', () => {
    render(
      <MessageBubble
        message={assistantMessage({
          content: '报告已整理完毕。',
          segments: [],
          artifacts: [
            {
              name: 'AI 行业近期资讯总结报告.md',
              mimeType: 'text/markdown',
              dataUrl: 'data:text/markdown;base64,IyBSZXBvcnQKCkRvbmUu',
              sizeBytes: 28,
              path: '/Users/test/Kivio/runs/conv_1/msg_1/report.md',
            },
          ],
        })}
      />,
    )

    expect(screen.getByRole('button', { name: /打开文件 AI 行业近期资讯总结报告\.md/ })).toBeInTheDocument()
    expect(screen.getByText('Markdown · 28 B')).toBeInTheDocument()
  })

  it('keeps image artifacts out of generated file cards', () => {
    render(
      <MessageBubble
        message={assistantMessage({
          content: '',
          segments: [],
          artifacts: [
            {
              name: 'chart.png',
              mimeType: 'image/png',
              dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
              sizeBytes: 8,
            },
          ],
        })}
      />,
    )

    expect(screen.queryByRole('button', { name: /打开文件 chart\.png/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '预览图片' })).toBeInTheDocument()
  })

  it('scopes Thinking duration to each reasoning segment in one assistant message', () => {
    render(
      <MessageBubble
        message={assistantMessage({
          segments: [
            {
              id: 'reasoning-1',
              kind: 'reasoning',
              phase: 'plain',
              order: 0,
              text: 'first thought',
            },
            {
              id: 'reasoning-2',
              kind: 'reasoning',
              phase: 'synthesis',
              order: 1,
              text: 'second thought',
            },
          ],
        })}
        reasoningDurationMsBySegmentId={{
          'reasoning-1': 11_000,
          'reasoning-2': 22_000,
        }}
      />,
    )

    const thinkingBlocks = screen.getAllByLabelText('Thinking')
    expect(thinkingBlocks).toHaveLength(2)
    expect(within(thinkingBlocks[0]).getByRole('button', { name: /Thinking/i })).toHaveTextContent('11s')
    expect(within(thinkingBlocks[1]).getByRole('button', { name: /Thinking/i })).toHaveTextContent('22s')
  })

  it('falls back to message-level Thinking duration only for a single reasoning segment', () => {
    render(
      <MessageBubble
        message={assistantMessage()}
        reasoningDurationMs={11_000}
      />,
    )

    const thinkingBlock = screen.getByLabelText('Thinking')
    expect(within(thinkingBlock).getByRole('button', { name: /Thinking/i })).toHaveTextContent('11s')
  })

  it('does not apply one message-level duration to multiple reasoning segments', () => {
    render(
      <MessageBubble
        message={assistantMessage({
          segments: [
            {
              id: 'reasoning-1',
              kind: 'reasoning',
              phase: 'plain',
              order: 0,
              text: 'first thought',
            },
            {
              id: 'reasoning-2',
              kind: 'reasoning',
              phase: 'synthesis',
              order: 1,
              text: 'second thought',
            },
          ],
        })}
        reasoningDurationMs={50_000}
      />,
    )

    for (const thinkingBlock of screen.getAllByLabelText('Thinking')) {
      expect(within(thinkingBlock).getByRole('button', { name: /Thinking/i })).not.toHaveTextContent('50s')
    }
  })

  it('keeps separate Thinking durations for multiple assistant messages in one conversation', () => {
    const firstMessage = assistantMessage({ id: 'assistant-1' })
    const secondMessage = assistantMessage({
      id: 'assistant-2',
      segments: [
        {
          id: 'reasoning-2',
          kind: 'reasoning',
          phase: 'plain',
          order: 0,
          text: 'second message thought',
        },
      ],
    })

    render(
      <MessageList
        conversationId="conversation-1"
        messages={[firstMessage, secondMessage]}
        assistantStreamStatsByMessageId={{
          'assistant-1': {
            messageId: 'assistant-1',
            tokensPerSec: 10,
            reasoningDurationMsBySegmentId: { 'reasoning-1': 11_000 },
          },
          'assistant-2': {
            messageId: 'assistant-2',
            tokensPerSec: 20,
            reasoningDurationMsBySegmentId: { 'reasoning-2': 22_000 },
          },
        }}
      />,
    )

    const thinkingBlocks = screen.getAllByLabelText('Thinking')
    expect(thinkingBlocks).toHaveLength(2)
    expect(within(thinkingBlocks[0]).getByRole('button', { name: /Thinking/i })).toHaveTextContent('11s')
    expect(within(thinkingBlocks[1]).getByRole('button', { name: /Thinking/i })).toHaveTextContent('22s')
  })
})
