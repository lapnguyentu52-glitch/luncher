import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import {
  clearBrowserFallbacks,
  registerBrowserFallback,
  requestCommand,
} from '@/services/ipc'
import { cancelTask, completeTask, setTaskProgress, spawnTask } from '@/services/coreCommands'
import type { CoreTask } from '@/types/commands'

/** Fake task registry phía browser fallback — mô phỏng lifecycle §90. */
const fakeTasks = new Map<string, CoreTask>()
let seq = 0

function resetFakes(): void {
  fakeTasks.clear()
  seq = 0
  registerBrowserFallback('core_spawn_task', (_cmd, args) => {
    const a = args as { kind: string; priority: string; dedupeKey?: string }
    seq += 1
    const task: CoreTask = {
      id: `task_${seq}`,
      kind: a.kind,
      state: 'QUEUED',
      priority: a.priority as CoreTask['priority'],
      progress: 0,
      cancellable: true,
    }
    if (a.dedupeKey !== undefined) task.dedupeKey = a.dedupeKey
    fakeTasks.set(task.id, task)
    return { ok: true, data: task, warnings: [] }
  })
  registerBrowserFallback('core_task_progress', (_cmd, args) => {
    const a = args as { taskId: string; progress: number; message?: string }
    const task = fakeTasks.get(a.taskId)
    if (task === undefined) {
      return { ok: false, error: { code: 'TASK_NOT_FOUND', message: a.taskId, retryable: false }, warnings: [] }
    }
    task.progress = Math.min(1, Math.max(0, a.progress))
    task.state = 'RUNNING'
    return { ok: true, data: task, warnings: [] }
  })
  registerBrowserFallback('core_complete_task', (_cmd, args) => {
    const a = args as { taskId: string }
    const task = fakeTasks.get(a.taskId)
    if (task === undefined) {
      return { ok: false, error: { code: 'TASK_NOT_FOUND', message: a.taskId, retryable: false }, warnings: [] }
    }
    task.state = 'COMPLETED'
    task.finishedAtMs = Date.now()
    return { ok: true, data: task, warnings: [] }
  })
  registerBrowserFallback('core_cancel_task', (_cmd, args) => {
    const a = args as { taskId: string }
    const task = fakeTasks.get(a.taskId)
    if (task === undefined) {
      return { ok: false, error: { code: 'TASK_NOT_FOUND', message: a.taskId, retryable: false }, warnings: [] }
    }
    task.state = task.state === 'QUEUED' ? 'CANCELLED' : 'CANCEL_REQUESTED'
    return { ok: true, data: task, warnings: [] }
  })
}

describe('core commands lifecycle (M4 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    resetFakes()
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('spawn → progress → complete', async () => {
    const task = await spawnTask('download.mod', 'P1_USER_ACTION')
    expect(task.state).toBe('QUEUED')

    const running = await setTaskProgress(task.id, 0.5, 'downloading')
    expect(running.state).toBe('RUNNING')
    expect(running.progress).toBe(0.5)

    const done = await completeTask(task.id)
    expect(done.state).toBe('COMPLETED')
    expect(done.finishedAtMs).toBeDefined()
  })

  it('spawn với dedupeKey giữ metadata để test §169 phía UI', async () => {
    const a = await spawnTask('download.mod', 'P1_USER_ACTION', 'file:x.zip')
    expect(a.dedupeKey).toBe('file:x.zip')
  })

  it('task không tồn tại → TASK_NOT_FOUND envelope', async () => {
    const res = await requestCommand('core_complete_task', { taskId: 'task_999' })
    expect(res.ok).toBe(false)
    expect(res.error?.code).toBe('TASK_NOT_FOUND')
  })

  it('cancel từ QUEUED → CANCELLED ngay', async () => {
    const task = await spawnTask('prefetch', 'P3_PREFETCH')
    const cancelled = await cancelTask(task.id)
    expect(cancelled.state).toBe('CANCELLED')
  })
})
