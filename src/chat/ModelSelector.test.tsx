import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ModelSelector } from './ModelSelector'

const { getSettingsCachedMock, subscribeSettingsMock, listModelsMock } = vi.hoisted(() => ({
  getSettingsCachedMock: vi.fn(),
  subscribeSettingsMock: vi.fn(),
  listModelsMock: vi.fn(),
}))

const { getCachedPolicyMock, syncPolicyMock } = vi.hoisted(() => ({
  getCachedPolicyMock: vi.fn(),
  syncPolicyMock: vi.fn(),
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

vi.mock('./modelRoutingPolicy', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./modelRoutingPolicy')>()
  return { ...actual, getModelRoutingPolicy: getCachedPolicyMock, syncModelRoutingPolicy: syncPolicyMock }
})

describe('ModelSelector', () => {
  beforeEach(() => {
    window.localStorage.clear()
    getSettingsCachedMock.mockResolvedValue({ runtimeMode: 'cloud', providers: [], favoriteModels: [] })
    listModelsMock.mockResolvedValue({ models: ['gpt-4o'], recommended: 'gpt-4o' })
    subscribeSettingsMock.mockReturnValue(() => {})
    getCachedPolicyMock.mockResolvedValue(null)
    syncPolicyMock.mockResolvedValue(null)
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

  it('filters the advanced list by quality tier and can show all models', async () => {
    const policy = {
      version: 3, updated_at: 1, recommended: {}, fallbacks: {},
      rules: [
        { model: 'fast-one', enabled: true, healthy: true, tiers: ['fast'], task_scores: { general: 80, creative: 60, coding: 60, reasoning: 50, vision: 0 }, capabilities: {}, multilingual: true, cost_level: 'low', priority: 0, fallback_priority: 0, rollout_percent: 100, updated_at: 1, version: 3 },
        { model: 'quality-one', enabled: true, healthy: true, tiers: ['quality'], task_scores: { general: 90, creative: 95, coding: 90, reasoning: 90, vision: 0 }, capabilities: {}, multilingual: true, cost_level: 'high', priority: 0, fallback_priority: 0, rollout_percent: 100, updated_at: 1, version: 3 },
      ],
    }
    listModelsMock.mockResolvedValue({ models: ['fast-one', 'quality-one'], recommended: 'quality-one' })
    getCachedPolicyMock.mockResolvedValue(policy)
    syncPolicyMock.mockResolvedValue(policy)
    render(<ModelSelector currentProviderId="abu-api-relay" currentModel="fast-one" onModelChange={() => {}} />)
    await waitFor(() => expect(listModelsMock).toHaveBeenCalled())
    fireEvent.click(screen.getByRole('button'))
    fireEvent.click(screen.getByText('高质量'))
    fireEvent.click(screen.getByText('高级：手动指定模型'))
    expect(screen.getByText('quality-one')).toBeInTheDocument()
    expect(screen.queryByText('fast-one')).not.toBeInTheDocument()
    fireEvent.click(screen.getByLabelText('显示全部模型'))
    expect(screen.getByText('fast-one')).toBeInTheDocument()
  })

})
