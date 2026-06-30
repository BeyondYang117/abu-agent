import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  beginGroup,
  endGroup,
  ensureGroupColumn,
  flushGroups,
  getActiveGroup,
  getGroupsVersion,
  hasActiveGroup,
  resetGroups,
  subscribeGroups,
  touchGroup,
} from './groupStreamingStore'

afterEach(() => {
  resetGroups()
  vi.restoreAllMocks()
})

describe('groupStreamingStore', () => {
  it('beginGroup 建出 N 个占位列并登记会话', () => {
    beginGroup('c1', 'g1', [
      { providerId: 'p1', model: 'm1' },
      { providerId: 'p2', model: 'm2' },
    ])
    expect(hasActiveGroup('c1')).toBe(true)
    const group = getActiveGroup('c1')
    expect(group?.groupId).toBe('g1')
    expect(group?.expectedColumns).toBe(2)
    expect(group?.columns).toHaveLength(2)
    expect(group?.columns[0].providerId).toBe('p1')
    expect(group?.columns[1].model).toBe('m2')
  })

  it('ensureGroupColumn 第一次见到 messageId 时认领占位列、绑定真实 id', () => {
    beginGroup('c1', 'g1', [
      { providerId: 'p1', model: 'm1' },
      { providerId: 'p2', model: 'm2' },
    ])
    const colA = ensureGroupColumn('c1', 'msg_a')
    const colB = ensureGroupColumn('c1', 'msg_b')
    expect(colA?.messageId).toBe('msg_a')
    expect(colB?.messageId).toBe('msg_b')
    // 两次认领的是不同的占位列。
    expect(colA?.providerId).toBe('p1')
    expect(colB?.providerId).toBe('p2')
    // 再次以同 id 取回同一列（按 messageId 聚合，同一会话多条流并存）。
    const colAagain = ensureGroupColumn('c1', 'msg_a')
    expect(colAagain).toBe(colA)
  })

  it('多条流靠 messageId 区分、各自累积，互不串', () => {
    beginGroup('c1', 'g1', [
      { providerId: 'p1', model: 'm1' },
      { providerId: 'p2', model: 'm2' },
    ])
    const a = ensureGroupColumn('c1', 'msg_a')!
    const b = ensureGroupColumn('c1', 'msg_b')!
    a.content += 'hello from A'
    b.content += 'hi from B'
    expect(getActiveGroup('c1')?.columns.find((c) => c.messageId === 'msg_a')?.content).toBe('hello from A')
    expect(getActiveGroup('c1')?.columns.find((c) => c.messageId === 'msg_b')?.content).toBe('hi from B')
  })

  it('未知会话的 ensureGroupColumn 返回 null（单模型路径不受影响）', () => {
    expect(ensureGroupColumn('no-group', 'msg_x')).toBeNull()
  })

  it('endGroup 清掉活跃组', () => {
    beginGroup('c1', 'g1', [{ providerId: 'p1', model: 'm1' }])
    endGroup('c1')
    expect(hasActiveGroup('c1')).toBe(false)
    expect(getActiveGroup('c1')).toBeUndefined()
  })

  it('touchGroup 合帧：N 个 delta 只通知一次（性能）；flushGroups 立即 flush', () => {
    // 用假的 rAF 控制何时执行合帧回调。
    const rafCallbacks: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      rafCallbacks.push(cb)
      return rafCallbacks.length
    })
    vi.stubGlobal('cancelAnimationFrame', () => {})

    beginGroup('c1', 'g1', [{ providerId: 'p1', model: 'm1' }])
    const col = ensureGroupColumn('c1', 'msg_a')!
    const sub = vi.fn()
    const unsub = subscribeGroups(sub)
    const versionBefore = getGroupsVersion()

    // 多个 delta：内容即时累积，但只调度一帧（不立即通知）。
    col.content += 'a'
    touchGroup()
    col.content += 'b'
    touchGroup()
    col.content += 'c'
    touchGroup()
    expect(sub).not.toHaveBeenCalled()
    expect(getGroupsVersion()).toBe(versionBefore)

    // 执行合帧帧：只通知一次。
    rafCallbacks.forEach((cb) => cb(0))
    expect(sub).toHaveBeenCalledTimes(1)
    expect(getActiveGroup('c1')?.columns[0].content).toBe('abc')

    // flushGroups 立即 flush 待合帧的更新。
    rafCallbacks.length = 0
    col.content += 'd'
    touchGroup()
    flushGroups()
    expect(sub).toHaveBeenCalledTimes(2)

    unsub()
  })
})
