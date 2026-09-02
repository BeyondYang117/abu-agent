import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { SourcesButton } from './SourcesButton'

function renderSources(platformWebSearchSupported: boolean) {
  render(
    <SourcesButton
      knowledgeBaseIds={[]}
      onChangeKnowledgeBaseIds={vi.fn()}
      mcpServers={[]}
      onToggleMcpServer={vi.fn()}
      webSearchMode="off"
      onSetWebSearchMode={vi.fn()}
      builtinWebSearchSupported={false}
      platformWebSearchSupported={platformWebSearchSupported}
    />,
  )
  fireEvent.click(screen.getByRole('button', { name: /信息来源/ }))
  return screen.getByRole('button', { name: '平台' })
}

describe('SourcesButton platform search', () => {
  it('enables platform search in cloud runtime', () => {
    expect(renderSources(true)).toBeEnabled()
  })

  it('disables platform search outside cloud runtime', () => {
    const button = renderSources(false)
    expect(button).toBeDisabled()
    expect(button).toHaveAttribute('title', '平台搜索仅在云端模式可用')
  })
})
