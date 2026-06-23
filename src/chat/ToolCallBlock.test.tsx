import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { ToolCallBlock } from './ToolCallBlock'
import type { ToolCallRecord } from './types'

function buildToolCall(overrides: Partial<ToolCallRecord> = {}): ToolCallRecord {
  return {
    id: 'tool-1',
    toolName: 'read_file',
    status: 'success',
    result_preview: 'file contents loaded',
    ...overrides,
  }
}

describe('ToolCallBlock', () => {
  it('renders success status with localized tool label', () => {
    render(<ToolCallBlock toolCall={buildToolCall()} />)
    expect(screen.getByRole('button', { name: /read_file/i })).toBeInTheDocument()
    expect(screen.getByText(/已完成/)).toBeInTheDocument()
  })

  it('does not style row preview as error when status is success but error field is present', () => {
    render(
      <ToolCallBlock
        toolCall={buildToolCall({
          status: 'success',
          error: 'legacy warning text',
          result_preview: 'ok',
        })}
      />,
    )
    const button = screen.getByRole('button', { name: /read_file/i })
    const preview = within(button).getByText(/legacy warning text/)
    expect(preview.className).not.toContain('text-red-500')
  })

  it('styles row preview as error only for error status', () => {
    render(
      <ToolCallBlock
        toolCall={buildToolCall({
          status: 'error',
          error: 'permission denied',
        })}
      />,
    )
    const button = screen.getByRole('button', { name: /read_file/i })
    const preview = within(button).getByText(/permission denied/)
    expect(preview.className).toContain('text-red-500')
  })

  it('expands details when clicked', async () => {
    const user = userEvent.setup()
    render(
      <ToolCallBlock
        toolCall={buildToolCall({
          arguments: { path: 'README.md' },
        })}
        defaultOpen={false}
      />,
    )
    await user.click(screen.getByRole('button', { name: /read_file/i }))
    expect(screen.getByText('参数')).toBeInTheDocument()
    expect(screen.getByText(/README\.md/)).toBeInTheDocument()
  })

  it('formats grep preview with query and file path', () => {
    render(
      <ToolCallBlock
        toolCall={buildToolCall({
          toolName: 'grep',
          result_preview: '',
          arguments: {
            query: 'ClaudeAgentClient',
            path: 'packages/server/src/server/agent/providers/claude/agent.ts',
          },
        })}
      />,
    )
    const button = screen.getByRole('button', { name: /grep/i })
    expect(within(button).getByText(/搜索 ClaudeAgentClient/)).toBeInTheDocument()
    expect(within(button).getByText(/providers\/claude\/agent\.ts/)).toBeInTheDocument()
  })

  it('formats grep preview with query and glob scope', () => {
    render(
      <ToolCallBlock
        toolCall={buildToolCall({
          toolName: 'grep',
          result_preview: '',
          arguments: {
            pattern: 'ClaudeAgentClient',
            glob: '**/claude/agent.ts',
          },
        })}
      />,
    )
    const button = screen.getByRole('button', { name: /grep/i })
    expect(within(button).getByText(/搜索 ClaudeAgentClient/)).toBeInTheDocument()
    expect(within(button).getByText(/\.\s\+\s\*\*\/claude\/agent\.ts/)).toBeInTheDocument()
  })

  it('falls back to stored grep argument preview when parsed arguments are unavailable', () => {
    render(
      <ToolCallBlock
        toolCall={buildToolCall({
          toolName: 'grep',
          result_preview: '',
          arguments: '{"query":',
          argumentPreview: '正在生成工具参数…',
          argumentsPreview: '正在生成工具参数…',
        })}
      />,
    )
    const button = screen.getByRole('button', { name: /grep/i })
    expect(within(button).getByText(/正在生成工具参数/)).toBeInTheDocument()
    expect(within(button).queryByText(/搜索文本/)).not.toBeInTheDocument()
  })
})
