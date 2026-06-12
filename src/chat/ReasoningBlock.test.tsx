import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ReasoningBlock } from './ReasoningBlock'

vi.mock('./ChatMarkdown', () => ({
  ChatMarkdown: ({ content }: { content: string }) => (
    <div data-testid="chat-markdown">{content}</div>
  ),
}))

describe('ReasoningBlock', () => {
  it('renders section shell with empty markdown body for empty reasoning', () => {
    render(<ReasoningBlock reasoning="" />)
    const section = screen.getByLabelText('Thinking')
    expect(within(section).getByTestId('chat-markdown')).toHaveTextContent('')
  })

  it('shows thinking title and collapsed preview while streaming', () => {
    render(<ReasoningBlock reasoning={'line one\nline two\nline three\nline four'} streaming />)
    const section = screen.getByLabelText('Thinking')
    expect(within(section).getByRole('button', { name: /Thinking/i })).toBeInTheDocument()
    expect(within(section).getByTestId('chat-markdown')).toHaveTextContent('line four')
  })

  it('expands full reasoning after toggle', async () => {
    const user = userEvent.setup()
    const reasoning = 'alpha\nbeta\ngamma'
    render(<ReasoningBlock reasoning={reasoning} streaming />)
    const section = screen.getByLabelText('Thinking')
    await user.click(within(section).getByRole('button', { name: /Thinking/i }))
    const markdown = within(section).getByTestId('chat-markdown')
    expect(markdown.textContent).toContain('alpha')
    expect(markdown.textContent).toContain('beta')
    expect(markdown.textContent).toContain('gamma')
  })

  it('hides markdown body after streaming completes while collapsed', () => {
    const { rerender } = render(<ReasoningBlock reasoning="done thinking" streaming />)
    rerender(<ReasoningBlock reasoning="done thinking" streaming={false} />)
    const section = screen.getByLabelText('Thinking')
    expect(within(section).getByRole('button', { name: /Thinking/i })).toBeInTheDocument()
    expect(within(section).getByTestId('chat-markdown').parentElement).toHaveAttribute('aria-hidden', 'true')
  })
})
