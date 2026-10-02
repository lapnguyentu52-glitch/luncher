/**
 * B10 — Game + System Optimization (§3/§4/§70/§35–§37).
 * Mirror services/optimization/service.py + services/system/service.py payloads.
 */

/** Optimization profile catalogue entry (mirror profiles.PROFILES). */
export interface OptimizationProfileEntry {
  id: string
  labelKey: string
  descKey: string
}

/** §3.1 — scan result: hardware + recommendation + instance hiện tại. */
export interface OptimizationScan {
  hardware: { ramTotalMb: number; cpuThreads: number }
  memory: Record<string, unknown>
  profile: string
  warnings: string[]
  bottlenecks: string[]
  instance: {
    id: string
    name?: string
    memory?: Record<string, unknown>
    jvmPreset?: string
  } | null
  currentProfile: string | null
  profiles: OptimizationProfileEntry[]
}

/** Một dòng diff trong plan preview (§3.1 — Before / After). */
export interface OptimizationChange {
  field: string
  before: unknown
  after: unknown
}

/** §3.1/§70 — plan preview: không ghi gì cho tới apply. */
export interface OptimizationPlan {
  instanceId: string
  profileId: string
  jvm: OptimizationChange[]
  minecraft: OptimizationChange[]
  hasChanges: boolean
}

/** §70 — snapshot info (rollback 1 click). */
export interface OptimizationSnapshotInfo {
  file: string
  appliedAt?: number
  profile?: string | null
  instanceFields: string[]
}

export interface OptimizationApplyResult {
  snapshot: { file: string; appliedAt: number }
  plan: OptimizationPlan
  profile: string
}

/** §35 — cleanup item trong preview (risk/reversible ở service). */
export interface CleanupItem {
  path: string
  label?: string
  bytes?: number
  risk?: string
  reversible?: boolean
  requiresAdmin?: boolean
  [key: string]: unknown
}

export interface CleanupPreview {
  items: CleanupItem[]
  totalBytes?: number
  [key: string]: unknown
}

export interface CleanupResult {
  bytes?: number
  cleanId?: string
  [key: string]: unknown
}

/** §36 — power plan status. */
export interface PowerStatus {
  plan: string
  plans?: Array<{ id: string; labelKey?: string }>
  [key: string]: unknown
}

/** §4.2 — system overview: hardware + power + cleanup preview. */
export interface SystemOverview {
  hardware: {
    ramTotalMb: number
    cpuThreads: number
    ramUsedMb?: number
    ramPercent?: number
    battery?: { percent: number; plugged: boolean }
  }
  power: PowerStatus
  cleanupPreview: CleanupPreview
}
