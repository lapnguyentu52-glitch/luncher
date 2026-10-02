import { invokeCommand } from './ipc'
import type {
  CoreStatusPayload,
  CoreTask,
  CoreTaskPriority,
} from '@/types/commands'

export async function getCoreStatus(): Promise<CoreStatusPayload> {
  return invokeCommand('core_status')
}

export async function spawnTask(
  kind: string,
  priority: CoreTaskPriority = 'P1_USER_ACTION',
  dedupeKey?: string,
): Promise<CoreTask> {
  const args: { kind: string; priority: CoreTaskPriority; dedupeKey?: string } = { kind, priority }
  if (dedupeKey !== undefined) args.dedupeKey = dedupeKey
  return invokeCommand('core_spawn_task', args)
}

export async function setTaskProgress(
  taskId: string,
  progress: number,
  message?: string,
): Promise<CoreTask> {
  const args: { taskId: string; progress: number; message?: string } = { taskId, progress }
  if (message !== undefined) args.message = message
  return invokeCommand('core_task_progress', args)
}

export async function completeTask(taskId: string): Promise<CoreTask> {
  return invokeCommand('core_complete_task', { taskId })
}

export async function cancelTask(taskId: string): Promise<CoreTask> {
  return invokeCommand('core_cancel_task', { taskId })
}
