import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { GitDiffStat, GitRepoState } from './types'

const gitStatus = vi.fn<(workdir: string) => Promise<GitRepoState>>()
const gitDiffStat = vi.fn<(workdir: string) => Promise<GitDiffStat>>()

vi.mock('./api', () => ({
  dockApi: {
    gitStatus: (workdir: string) => gitStatus(workdir),
    gitDiffStat: (workdir: string) => gitDiffStat(workdir),
  },
}))

const { fetchGitBadge } = await import('./useGitBadge')

function repoState(head: string): GitRepoState {
  return {
    repoRoot: '/repo',
    head,
    upstream: null,
    ahead: 0,
    behind: 0,
    stashCount: 0,
    entries: [],
    status: 'ready',
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => { resolve = r })
  return { promise, resolve }
}

describe('fetchGitBadge', () => {
  beforeEach(() => {
    gitStatus.mockReset()
    gitDiffStat.mockReset()
    gitDiffStat.mockResolvedValue({ filesChanged: 0, additions: 0, deletions: 0, files: [] })
  })

  it('coalesces overlapping refreshes into one trailing rerun', async () => {
    const first = deferred<GitRepoState>()
    gitStatus.mockReturnValueOnce(first.promise).mockResolvedValueOnce(repoState('second'))

    const a = fetchGitBadge('/repo')
    // 进行中又来三次：只补跑一次，且所有人拿到补跑后的结果。
    const b = fetchGitBadge('/repo')
    const c = fetchGitBadge('/repo')
    const d = fetchGitBadge('/repo')
    first.resolve(repoState('first'))

    const results = await Promise.all([a, b, c, d])
    expect(gitStatus).toHaveBeenCalledTimes(2)
    expect(results.every((r) => r.state.head === 'second')).toBe(true)
  })

  it('runs again once the previous fetch has settled', async () => {
    gitStatus.mockResolvedValue(repoState('main'))
    await fetchGitBadge('/repo')
    await fetchGitBadge('/repo')
    expect(gitStatus).toHaveBeenCalledTimes(2)
  })

  it('keeps workdirs independent and skips diff stat outside a repo', async () => {
    gitStatus.mockImplementation(async (workdir) =>
      workdir === '/plain' ? { ...repoState(''), status: 'not_repo' } : repoState('main'),
    )
    const [repo, plain] = await Promise.all([fetchGitBadge('/repo'), fetchGitBadge('/plain')])
    expect(gitStatus).toHaveBeenCalledTimes(2)
    expect(gitDiffStat).toHaveBeenCalledTimes(1)
    expect(repo.diffStat).not.toBeNull()
    expect(plain.diffStat).toBeNull()
  })
})
