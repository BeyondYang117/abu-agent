// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  MULTI_ANSWER_VIEW_STORAGE_KEY,
  _resetMultiAnswerViewModeForTest,
  useMultiAnswerViewMode,
} from './multiAnswerViewMode'

beforeEach(() => {
  window.localStorage.clear()
  _resetMultiAnswerViewModeForTest()
})

afterEach(() => {
  window.localStorage.clear()
  _resetMultiAnswerViewModeForTest()
})

describe('useMultiAnswerViewMode', () => {
  it('默认 tabs', () => {
    const { result } = renderHook(() => useMultiAnswerViewMode())
    expect(result.current[0]).toBe('tabs')
  })

  it('setMode 写入 localStorage 并更新返回值', () => {
    const { result } = renderHook(() => useMultiAnswerViewMode())
    act(() => {
      result.current[1]('columns')
    })
    expect(result.current[0]).toBe('columns')
    expect(window.localStorage.getItem(MULTI_ANSWER_VIEW_STORAGE_KEY)).toBe('columns')
  })

  it('多个订阅者即时同步（全局偏好）', () => {
    const a = renderHook(() => useMultiAnswerViewMode())
    const b = renderHook(() => useMultiAnswerViewMode())
    act(() => {
      a.result.current[1]('columns')
    })
    expect(a.result.current[0]).toBe('columns')
    expect(b.result.current[0]).toBe('columns')
  })

  it('跨窗口同步：另一窗口改偏好 → storage 事件同步本窗口', () => {
    const { result } = renderHook(() => useMultiAnswerViewMode())
    expect(result.current[0]).toBe('tabs')
    act(() => {
      window.localStorage.setItem(MULTI_ANSWER_VIEW_STORAGE_KEY, 'columns')
      window.dispatchEvent(new StorageEvent('storage', {
        key: MULTI_ANSWER_VIEW_STORAGE_KEY,
        newValue: 'columns',
      }))
    })
    expect(result.current[0]).toBe('columns')
  })

  it('非法值回退默认 tabs', () => {
    window.localStorage.setItem(MULTI_ANSWER_VIEW_STORAGE_KEY, 'garbage')
    const { result } = renderHook(() => useMultiAnswerViewMode())
    act(() => {
      window.dispatchEvent(new StorageEvent('storage', {
        key: MULTI_ANSWER_VIEW_STORAGE_KEY,
        newValue: 'garbage',
      }))
    })
    expect(result.current[0]).toBe('tabs')
  })
})
