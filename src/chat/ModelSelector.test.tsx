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
    expect(screen.getAllByText('gpt-4o')).toHaveLength(2)

    act(() => {
      onSettingsUpdated({ runtimeMode: 'cloud', providers: [], favoriteModels: [] })
    })

    expect(screen.getAllByText('gpt-4o')).toHaveLength(2)
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
    expect(screen.getAllByText('视觉')).toHaveLength(1)
  })

})
