// Git 徽标共享数据源：dock_git_status + dock_git_diff_stat，
// workspace:activity 秒级驱动 + watcher 不可用时 10s 兜底轮询。
// 签名相同跳过 setState；GitStatusPill（工具栏）与 GitDiffChip（状态条）各自实例化，
// 换取两个组件互不耦合 —— 实际的 git 调用由 fetchGitBadge 按 workdir 合并，不翻倍。
import { useCallback, useEffect, useRef, useState } from 'react'
import { dockApi } from './api'
import { gitStatusSignature } from './gitReviewModel'
import type { GitDiffStat, GitRepoState } from './types'
import { workspaceActivity } from './workspaceActivity'

export type GitBadge = {
  state: GitRepoState | null
  diffStat: GitDiffStat | null
  loading: boolean
  /** 变更操作（如 init）返回的权威 state 直接写入，不等下一轮 refresh。 */
  applyMutationState: (next: GitRepoState) => void
  refresh: (options?: { silent?: boolean }) => Promise<void>
}

type GitBadgeSnapshot = { state: GitRepoState; diffStat: GitDiffStat | null }

const inflight = new Map<string, { promise: Promise<GitBadgeSnapshot>; requestRerun: () => void }>()

async function loadGitBadge(workdir: string): Promise<GitBadgeSnapshot> {
  const state = await dockApi.gitStatus(workdir)
  if (state.status !== 'ready') return { state, diffStat: null }
  const diffStat = await dockApi.gitDiffStat(workdir).catch(() => null)
  return { state, diffStat }
}

/** 同一 workdir 的并发刷新合并成一次 git 调用。两个徽标各自实例化本 hook，每批
 *  workspace:activity 以前是两遍 `git status` + `git diff --numstat`，且上一遍没跑完下一批
 *  又来 —— Windows 上每次都是起 git.exe。进行中又来的请求只记一笔，结束后补跑一次，
 *  所有等待者拿到的都是补跑后的最新结果。 */
export function fetchGitBadge(workdir: string): Promise<GitBadgeSnapshot> {
  const running = inflight.get(workdir)
  if (running) {
    running.requestRerun()
    return running.promise
  }
  let rerun = false
  const promise = (async () => {
    try {
      let snapshot = await loadGitBadge(workdir)
      while (rerun) {
        rerun = false
        snapshot = await loadGitBadge(workdir)
      }
      return snapshot
    } finally {
      inflight.delete(workdir)
    }
  })()
  inflight.set(workdir, { promise, requestRerun: () => { rerun = true } })
  return promise
}

export function useGitBadge(workdir: string): GitBadge {
  const [state, setState] = useState<GitRepoState | null>(null)
  const [diffStat, setDiffStat] = useState<GitDiffStat | null>(null)
  const [loading, setLoading] = useState(false)
  const signatureRef = useRef('')

  const applyState = useCallback((next: GitRepoState) => {
    const signature = gitStatusSignature(next)
    if (signature === signatureRef.current) return
    signatureRef.current = signature
    setState(next)
  }, [])

  const applyMutationState = useCallback((next: GitRepoState) => {
    signatureRef.current = gitStatusSignature(next)
    setState(next)
  }, [])

  const refresh = useCallback(
    async (options?: { silent?: boolean }) => {
      if (!workdir) return
      if (!options?.silent) setLoading(true)
      try {
        const { state: next, diffStat: stat } = await fetchGitBadge(workdir)
        applyState(next)
        if (next.status === 'ready') {
          if (stat) {
            setDiffStat((prev) =>
              prev &&
              prev.filesChanged === stat.filesChanged &&
              prev.additions === stat.additions &&
              prev.deletions === stat.deletions
                ? prev
                : stat,
            )
          }
        } else {
          setDiffStat(null)
        }
      } catch {
        // 静默：状态类信息不打断用户，下一轮刷新会自愈。
      } finally {
        setLoading(false)
      }
    },
    [applyState, workdir],
  )

  // workdir 切换：重置签名缓存并立即拉取。
  useEffect(() => {
    signatureRef.current = ''
    setState(null)
    setDiffStat(null)
    void refresh()
  }, [refresh])

  // 外部改动（含 agent 写文件）驱动静默刷新；watcher 不可用时 10s 兜底轮询。
  useEffect(() => {
    if (!workdir) return
    const unsubscribe = workspaceActivity.subscribe(workdir, (event) => {
      if (event.fs || event.git || event.truncated) void refresh({ silent: true })
    })
    if (workspaceActivity.isAvailable()) return unsubscribe
    const timer = window.setInterval(() => void refresh({ silent: true }), 10_000)
    return () => {
      unsubscribe()
      window.clearInterval(timer)
    }
  }, [refresh, workdir])

  return { state, diffStat, loading, applyMutationState, refresh }
}
