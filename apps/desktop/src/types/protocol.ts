/**
 * Antares Typed Protocol — Batch 3
 *
 * Single source of truth cho contract giữa Vue UI và Rust core.
 * Rust side: src-tauri/src/protocol/*.rs — mọi thay đổi phải đồng bộ 2 phía.
 * Sau này có thể generate file này từ Rust (specta/ts-rs) — giữ shape ổn định.
 */

// ---------------------------------------------------------------------------
// §96 — Response envelope
// ---------------------------------------------------------------------------

export interface AntaresError {
  code: string
  message: string
  retryable: boolean
  action?: string
}

export interface AntaresResponse<T> {
  ok: boolean
  data?: T
  error?: AntaresError
  warnings: string[]
}

// ---------------------------------------------------------------------------
// §117 — Error taxonomy: catalog tập trung, UI không hardcode string lẻ
// ---------------------------------------------------------------------------

export const ErrorDomain = {
  App: 'APP',
  Config: 'CONFIG',
  Storage: 'STORAGE',
  Network: 'NETWORK',
  Process: 'PROCESS',
  Ipc: 'IPC',
  Mc: 'MC',
  Loader: 'LOADER',
  Mod: 'MOD',
  Resource: 'RESOURCE',
  Java: 'JAVA',
  Auth: 'AUTH',
  Disk: 'DISK',
  Ui: 'UI',
  Plugin: 'PLUGIN',
  Security: 'SECURITY',
} as const

export const ErrorCodes = {
  AppInternal: 'APP_INTERNAL',
  AppNotReady: 'APP_NOT_READY',
  ConfigInvalid: 'CONFIG_INVALID',
  StorageWriteFailed: 'STORAGE_WRITE_FAILED',
  NetworkUnavailable: 'NETWORK_UNAVAILABLE',
  IpcSessionStale: 'IPC_SESSION_STALE',
  InstanceLocked: 'INSTANCE_LOCKED',
  JavaNotFound: 'JAVA_NOT_FOUND',
  McVersionUnknown: 'MC_VERSION_UNKNOWN',
  LegacyDisabled: 'LEGACY_DISABLED',
  InstanceNameInvalid: 'INSTANCE_NAME_INVALID',
} as const

export type ErrorCode = (typeof ErrorCodes)[keyof typeof ErrorCodes]

export function errorDomain(code: string): string {
  return code.split('_')[0] ?? 'UNKNOWN'
}

// ---------------------------------------------------------------------------
// §88.3 / §32 — Event envelope + QoS
// ---------------------------------------------------------------------------

export const EventTopic = {
  App: 'app',
  Runtime: 'runtime',
  Download: 'download',
  Diagnostics: 'diagnostics',
  Notification: 'notification',
  Telemetry: 'telemetry',
} as const
export type EventTopic = (typeof EventTopic)[keyof typeof EventTopic]

/** §32 — Event policies. */
export const EventQos = {
  Latest: 'latest',
  Coalesce: 'coalesce',
  Batched: 'batched',
  Lossless: 'lossless',
} as const
export type EventQos = (typeof EventQos)[keyof typeof EventQos]

export interface EventEnvelope<P = unknown> {
  id: string
  schema: number
  topic: EventTopic
  qos: EventQos
  name: string
  timestampMs: number
  correlationId?: string
  payload: P
}

/** Tên channel Tauri mà Rust emit envelope lên — giữ đồng bộ với commands/app.rs. */
export const EVENT_CHANNEL = 'antares://event'
export const EVENT_SCHEMA_VERSION = 1

export function isEventEnvelope(value: unknown): value is EventEnvelope {
  if (typeof value !== 'object' || value === null) return false
  const v = value as Record<string, unknown>
  return (
    typeof v.id === 'string' &&
    typeof v.schema === 'number' &&
    typeof v.topic === 'string' &&
    typeof v.qos === 'string' &&
    typeof v.name === 'string' &&
    typeof v.timestampMs === 'number' &&
    v.payload !== undefined
  )
}
