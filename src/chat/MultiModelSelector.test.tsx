import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { MultiModelSelector } from './MultiModelSelector'

const { getSettingsCachedMock, listModelsMock } = vi.hoisted(() => ({
  getSettingsCachedMock: vi.fn(),
  listModelsMock: vi.fn(),
}))

vi.mock('../api/settingsCache', () => ({
  getSettingsCached: getSettingsCachedMock,
}))

vi.mock('../api/abuApi', () => ({
  listModels: listModelsMock,
  ABU_API_PROVIDER_ID: 'abu-api-relay',
}))

describe('MultiModelSelector', () => {
  beforeEach(() => {
    getSettingsCachedMock.mockReset()
    listModelsMock.mockReset()
  })

  it('云端模式从 ABU API 加载并选择并行模型', async () => {
    const onChange = vi.fn()
    getSettingsCachedMock.mockResolvedValue({ runtimeMode: 'cloud', providers: [] })
    listModelsMock.mockResolvedValue({
      models: ['gpt-5.6-sol', 'claude-haiku-4-5'],
      recommended: 'gpt-5.6-sol',
    })

    render(<MultiModelSelector value={[]} onChange={onChange} placement="down" />)
    fireEvent.click(screen.getByRole('button', { name: /并行回答/ }))

    await waitFor(() => expect(listModelsMock).toHaveBeenCalledOnce())
    expect(screen.getByText('ABU Cloud')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /gpt-5.6-sol/ }))
    expect(onChange).toHaveBeenCalledWith([
      { provider_id: 'abu-api-relay', model: 'gpt-5.6-sol' },
    ])
  })
})
