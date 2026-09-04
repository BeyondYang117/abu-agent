import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { SourcesButton } from './SourcesButton'

describe('SourcesButton search modes', () => {
  it('exposes platform search in cloud mode', () => {
    render(
      <SourcesButton
        knowledgeBaseIds={[]}
        onChangeKnowledgeBaseIds={vi.fn()}
        mcpServers={[]}
        onToggleMcpServer={vi.fn()}
        webSearchMode="off"
        onSetWebSearchMode={vi.fn()}
        builtinWebSearchSupported={false}
        platformWebSearchSupported
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: /信息来源/ }))
    expect(screen.getByRole('button', { name: '平台' })).toBeEnabled()
  })
})
