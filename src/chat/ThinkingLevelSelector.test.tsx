import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ThinkingLevelSelector } from './ThinkingLevelSelector'

const { reasoningEffortsForModel } = vi.hoisted(() => ({
  reasoningEffortsForModel: vi.fn(),
}))

// api 在 jsdom 无 Tauri 环境，mock 成确定值；等级清单走兜底也是同样结果。
vi.mock('../api/tauri', () => ({
  api: {
    getSettings: () => Promise.resolve({ providers: [] }),
    reasoningEffortsForModel,
  },
}))

describe('ThinkingLevelSelector', () => {
  beforeEach(() => {
    reasoningEffortsForModel.mockReset()
    reasoningEffortsForModel.mockResolvedValue(['low', 'medium', 'high'])
  })

  it('value=null 时默认显示自动推荐', () => {
    render(
      <ThinkingLevelSelector
        value={null}
        currentProviderId="p1"
        currentModel="m1"
        onChange={() => {}}
      />,
    )
    expect(screen.getByRole('button')).toHaveTextContent('自动（推荐）')
  })

  it('下拉项使用面向用户的中文名称和用途说明', () => {
    render(
      <ThinkingLevelSelector
        value="high"
        currentProviderId="p1"
        currentModel="m1"
        onChange={() => {}}
      />,
    )
    act(() => {
      fireEvent.click(screen.getByRole('button'))
    })
    expect(screen.getByText('自动（推荐）')).toBeInTheDocument()
    expect(screen.getByText('关闭思考')).toBeInTheDocument()
    expect(screen.getByText('均衡')).toBeInTheDocument()
    expect(screen.getByText('兼顾速度与效果，适合日常任务')).toBeInTheDocument()
  })

  it('选择某一档回调原始等级值', () => {
    const onChange = vi.fn()
    render(
      <ThinkingLevelSelector
        value="high"
        currentProviderId="p1"
        currentModel="m1"
        onChange={onChange}
      />,
    )
    act(() => {
      fireEvent.click(screen.getByRole('button'))
    })
    act(() => {
      fireEvent.click(screen.getByText('关闭思考'))
    })
    expect(onChange).toHaveBeenCalledWith('off')
  })

  it('选择自动推荐时回调 null', () => {
    const onChange = vi.fn()
    render(
      <ThinkingLevelSelector
        value="high"
        currentProviderId="p1"
        currentModel="m1"
        onChange={onChange}
      />,
    )
    fireEvent.click(screen.getByRole('button'))
    fireEvent.click(screen.getByText('自动（推荐）'))
    expect(onChange).toHaveBeenCalledWith(null)
  })

  it('能力列表加载完成前不会把 xhigh 回写成自动', async () => {
    let resolveLevels!: (levels: string[]) => void
    reasoningEffortsForModel.mockImplementationOnce(() => new Promise((resolve) => {
      resolveLevels = resolve
    }))
    const onChange = vi.fn()

    render(
      <ThinkingLevelSelector
        value="xhigh"
        currentProviderId="p1"
        currentModel="gpt-5.6-sol"
        onChange={onChange}
      />,
    )

    expect(onChange).not.toHaveBeenCalled()

    await act(async () => {
      resolveLevels(['low', 'medium', 'high', 'xhigh'])
    })
    expect(screen.getByRole('button')).toHaveTextContent('深度')
    expect(onChange).not.toHaveBeenCalled()
  })

  it('模型没有思考能力时不显示思考等级旋钮', async () => {
    reasoningEffortsForModel.mockResolvedValueOnce([])
    render(
      <ThinkingLevelSelector
        value="medium"
        currentProviderId="anthropic"
        currentModel="claude-haiku-4-5"
        onChange={() => {}}
      />,
    )

    await waitFor(() => {
      expect(screen.queryByRole('button')).not.toBeInTheDocument()
    })
  })
})
