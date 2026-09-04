import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ModelSelector } from './ModelSelector'

const { getSettingsCachedMock, subscribeSettingsMock, listModelsMock } = vi.hoisted(() => ({
  getSettingsCachedMock: vi.fn(),
  subscribeSettingsMock: vi.fn(),
  listModelsMock: vi.fn(),
}))

vi.mock('../api/settingsCache', () => ({
  getSettingsCached: getSettingsCachedMock,
  setFavoriteModelsCached: vi.fn(),
  subscribeSettings: subscribeSettingsMock,
}))

vi.mock('../api/abuApi', () => ({
  listModels: listModelsMock,
  ABU_API_PROVIDER_ID: 'abu-api-relay',
}))

describe('ModelSelector', () => {
  beforeEach(() => {
    window.localStorage.clear()
    getSettingsCachedMock.mockResolvedValue({ runtimeMode: 'cloud', providers: [], favoriteModels: [] })
    listModelsMock.mockResolvedValue({ models: ['gpt-4o'], recommended: 'gpt-4o' })
    subscribeSettingsMock.mockReturnValue(() => {})
  })

  it('keeps cloud models when a settings update arrives', async () => {
    let onSettingsUpdated!: (settings: { runtimeMode: string; providers: never[]; favoriteModels: never[] }) => void
    subscribeSettingsMock.mockImplementation((listener: typeof onSettingsUpdated) => {
      onSettingsUpdated = listener
      return () => {}
    })

    render(
      <ModelSelector
        currentProviderId="abu-api-relay"
        currentModel="gpt-4o"
        onModelChange={() => {}}
      />,
    )

    await waitFor(() => expect(listModelsMock).toHaveBeenCalledOnce())
    fireEvent.click(screen.getByRole('button'))
    fireEvent.click(screen.getByText('高级：手动指定模型'))
    expect(screen.getAllByText('gpt-4o')).toHaveLength(1)

    act(() => {
      onSettingsUpdated({ runtimeMode: 'cloud', providers: [], favoriteModels: [] })
    })

    expect(screen.getAllByText('gpt-4o')).toHaveLength(1)
  })

  it('shows the model ability tag in the dropdown', async () => {
    render(
      <ModelSelector
        currentProviderId="abu-api-relay"
        currentModel="gpt-4o"
        onModelChange={() => {}}
      />,
    )

    await waitFor(() => expect(listModelsMock).toHaveBeenCalled())
    fireEvent.click(screen.getByRole('button'))
    fireEvent.click(screen.getByText('高级：手动指定模型'))
    expect(screen.getAllByText('视觉')).toHaveLength(1)
  })

  it('defaults to smart selection and keeps manual models behind the advanced control', async () => {
    render(
      <ModelSelector
        currentProviderId="abu-api-relay"
        currentModel="gpt-4o"
        onModelChange={() => {}}
      />,
    )

    await waitFor(() => expect(listModelsMock).toHaveBeenCalled())
    expect(screen.getByRole('button')).toHaveTextContent('智能选择')
    fireEvent.click(screen.getByRole('button'))
    expect(screen.queryByText('gpt-4o')).not.toBeInTheDocument()
    expect(screen.getByText('快速')).toBeInTheDocument()
    expect(screen.getByText('均衡')).toBeInTheDocument()
    expect(screen.getByText('高质量')).toBeInTheDocument()
  })

})
