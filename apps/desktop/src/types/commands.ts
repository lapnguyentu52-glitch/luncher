import type { AntaresResponse } from './protocol'
import type { StorageMode } from './app'
import type { LegacyStatusPayload, LegacyVersionInfo } from './legacy'
import type {
  AccountSummary,
  AntaresInstance,
  DashboardSummary,
  JavaInfo,
  PreflightCheck,
  PreflightResult,
  PreflightStatus,
} from './instances'

/** Legacy launch task (core/tasks/manager.py to_dict). */
export interface LegacyTaskPayload {
  id: string
  type: string
  owner: string
  state: string
  progress: number
  message: string
  error?: { code: string; message: string }
}

/** Mirror của crates/antares-core/src/tasks.rs TaskState (§90). */
export type CoreTaskState =
  | 'QUEUED'
  | 'RUNNING'
  | 'PAUSED'
  | 'CANCEL_REQUESTED'
  | 'CANCELLED'
  | 'FAILED'
  | 'COMPLETED'

export type CoreTaskPriority = 'P0_CRITICAL' | 'P1_USER_ACTION' | 'P2_BACKGROUND' | 'P3_PREFETCH'

/** Mirror của crates/antares-core/src/tasks.rs Task (§90). */
export interface CoreTask {
  id: string
  kind: string
  dedupeKey?: string
  parentId?: string
  state: CoreTaskState
  priority: CoreTaskPriority
  progress: number
  message?: string
  cancellable: boolean
  startedAtMs?: number
  finishedAtMs?: number
}

/** Mirror của HubStats trong crates/antares-core/src/events.rs. */
export interface CoreHubStats {
  published: number
  dropped: number
  pendingLatest: number
  pendingCoalesce: number
  pendingBatched: number
  pendingLossless: number
}

export interface CoreStatusPayload {
  storageMode: StorageMode
  uptimeMs: number
  hub: CoreHubStats
  activeTasks: CoreTask[]
  /** M5 — ANTA_RUST_ONLY=1: legacy_call bị chặn ở Rust (LEGACY_DISABLED). */
  rustOnly: boolean
}

/**
 * §96 — Command registry: map command name → request/response type.
 * Thêm command mới = thêm 1 entry, UI gọi qua typed invoke ở services/ipc.ts.
 */
export interface CommandSchema {
  'app_ping': { request: void; response: PingPayload }
  'app_version': { request: void; response: VersionPayload }
  'app_storage_mode': { request: void; response: StorageMode }
  'app_storage_info': { request: void; response: StorageInfoPayload }
  'app_test_event': { request: void; response: TestEventPayload }
  'core_status': { request: void; response: CoreStatusPayload }
  'core_spawn_task': {
    request: { kind: string; priority: CoreTaskPriority; dedupeKey?: string }
    response: CoreTask
  }
  'core_task_progress': {
    request: { taskId: string; progress: number; message?: string }
    response: CoreTask
  }
  'core_complete_task': { request: { taskId: string }; response: CoreTask }
  'core_cancel_task': { request: { taskId: string }; response: CoreTask }
  // Batch 05 — native group instances (qua antares-app composition root)
  'instances_list': { request: void; response: { instances: AntaresInstance[] } }
  'instances_get': { request: { instanceId: string }; response: { instance: AntaresInstance } }
  'instances_create': {
    request: {
      name: string
      minecraftVersion: string
      loader?: string
      memoryMaxMb?: number
      memoryMinMb?: number
    }
    response: { instance: AntaresInstance }
  }
  'instances_select': { request: { instanceId: string }; response: { selected: string } }
  'legacy_status': { request: void; response: LegacyStatusPayload }
  'legacy_start': { request: void; response: LegacyVersionInfo }
  'legacy_call': {
    request: { method: string; params?: unknown; timeoutMs?: number }
    response: unknown
  }
  'legacy_restart': { request: void; response: LegacyVersionInfo }
  'legacy_shutdown': { request: void; response: { stopped: boolean } }
}

export type CommandName = keyof CommandSchema

export interface PingPayload {
  pong: boolean
  tsMs: number
}

export interface VersionPayload {
  app: string
  schema: number
}

export interface TestEventPayload {
  delivered: boolean
}

/** Mirror của StorageReport trong crates/antares-app/src/services.rs (F-14). */
export interface StorageInfoPayload {
  root: string
  scopes: { dir: string; exists: boolean }[]
}

/** Helper type: response envelope cho một command bất kỳ. */
export type CommandResponse<C extends CommandName> = AntaresResponse<CommandSchema[C]['response']>

// ---------------------------------------------------------------------------
// M6 — legacy domain payloads (qua legacy_call, sidecar methods)
// ---------------------------------------------------------------------------

/** Params cho legacy bridge domain calls. */
export interface LegacyDomainCall {
  method: string
  params?: Record<string, unknown>
  timeoutMs?: number
}

export type {
  AccountSummary,
  AntaresInstance,
  DashboardSummary,
  JavaInfo,
  PreflightCheck,
  PreflightResult,
  PreflightStatus,
}
