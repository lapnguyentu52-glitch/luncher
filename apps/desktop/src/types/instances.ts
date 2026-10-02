/** Mirror instance.json từ services/instances/service.py. */
export interface AntaresInstance {
  id: string
  name: string
  minecraftVersion: string
  loader: string
  directory: string
  memory: { minMb: number; maxMb: number }
  jvmArgs: string[]
  jvmPreset: string
  lastPlayedAt: number | null
  launchCount: number
}

/** Mirror services/java/discovery.py java_info(). */
export interface JavaInfo {
  path: string
  exe: string
  javaw?: string
  major: number
  name: string
}

/** §8/§30 — Preflight check result. */
export type PreflightStatus = 'pass' | 'warning' | 'error'

export interface PreflightCheck {
  id: string
  label: string
  status: PreflightStatus
  detail: string
}

export interface PreflightResult {
  instanceId: string
  checks: PreflightCheck[]
  blockers: number
  warnings: number
  canPlay: boolean
}

/** Mirror handle_dashboard_summary payload. */
export interface DashboardSummary {
  appVersion: string
  instanceCount: number
  selectedInstanceId: string | null
  recentInstanceId: string | null
  recentInstanceName: string | null
  account: { id: string; displayName: string } | null
  legacyAvailable: boolean
}

export interface AccountSummary {
  id: string
  displayName: string
  type?: string
}
