import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { Translator } from './App'

const apiMock = vi.hoisted(() => ({
  closeTranslatorWindow: vi.fn().mockResolvedValue(undefined),
  commitTranslation: vi.fn().mockResolvedValue(undefined),
  focusWindow: vi.fn().mockResolvedValue(undefined),
  translateText: vi.fn().mockResolvedValue('Hello world'),
}))

vi.mock('./api/tauri', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api/tauri')>()
  return { ...actual, api: { ...actual.api, ...apiMock } }
})

describe('Translator', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    apiMock.translateText.mockResolvedValue('Hello world')
  })

  it('keeps a multiline editor available while showing the result', async () => {
    const user = userEvent.setup()
    render(<Translator translateSource="test-model" lang="zh" onOpenSettings={vi.fn()} />)

    const input = screen.getByRole('textbox', { name: '输入或粘贴需要翻译的内容…' })
    await user.type(input, '你好世界')
    await user.click(screen.getByRole('button', { name: '立即翻译' }))

    expect(await screen.findByText('Hello world')).toBeVisible()
    expect(input).toBeVisible()
    expect(input.tagName).toBe('TEXTAREA')
    expect(screen.getByRole('button', { name: '使用译文' })).toBeEnabled()
  })

  it('uses Shift+Enter for a newline without translating', async () => {
    const user = userEvent.setup()
    render(<Translator translateSource="test-model" lang="zh" onOpenSettings={vi.fn()} />)

    const input = screen.getByRole('textbox', { name: '输入或粘贴需要翻译的内容…' })
    await user.type(input, '第一行{Shift>}{Enter}{/Shift}第二行')

    expect(input).toHaveValue('第一行\n第二行')
    expect(apiMock.translateText).not.toHaveBeenCalled()
  })

  it('translates immediately on Enter, then commits a fresh result on Enter', async () => {
    render(<Translator translateSource="test-model" lang="zh" onOpenSettings={vi.fn()} />)
    const input = screen.getByRole('textbox', { name: '输入或粘贴需要翻译的内容…' })

    fireEvent.change(input, { target: { value: '你好' } })
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' })
    await waitFor(() => expect(apiMock.translateText).toHaveBeenCalledWith('你好'))
    await screen.findByText('Hello world')

    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' })
    await waitFor(() => expect(apiMock.commitTranslation).toHaveBeenCalledWith('Hello world'))
  })
})
