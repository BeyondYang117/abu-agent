import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { SourcesButton } from './SourcesButton'

describe('SourcesButton search modes', () => {
  it('does not expose the retired platform search mode', () => {
    render(
      <SourcesButton
        knowledgeBaseIds={[]}
        onChangeKnowledgeBaseIds={vi.fn()}
        mcpServers={[]}
        onToggleMcpServer={vi.fn()}
        webSearchMode="off"
        onSetWebSearchMode={vi.fn()}
        builtinWebSearchSupported={false}
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: /信息来源/ }))
    expect(screen.queryByRole('button', { name: '平台' })).not.toBeInTheDocument()
  })
})
