/**
 * B13 — Diagnostics: log center + crash insights + repair + evidence (§41/§39/§16).
 * Mirror services/diagnostics/log_analyzer.py + services/repair/service.py payloads.
 */

/** Nguồn log: launcher ring / minecraft latest.log / crash-reports. */
export interface LogSourceInfo {
  id: 'launcher' | 'minecraft' | 'crash' | string
  available: boolean
  bytes: number
  lines: number
  count?: number
}

/** §16 — dòng log cho console ảo hoá. */
export interface LogLine {
  n: number
  text: string
}

export interface LogReadResult {
  source: string
  instanceId: string | null
  lines: LogLine[]
  truncated: boolean
}

/** §41 — insight từ blueprint (không LLM — mục 24). */
export interface LogInsight {
  id: string
  count: number
  severity: 'error' | 'warning' | string
  seed: string
  recommend: string
  action: { kind: string; target: string }
  firstSeen: string | null
  lastSeen: string | null
  sources: string[]
  lines: Array<{ source: string; line: number; text: string }>
}

export interface LogAnalysis {
  instanceId: string | null
  sources: Record<string, number>
  insights: LogInsight[]
  errorCount: number
  warnCount: number
  durationMs: number
}

export interface ConsoleInsights {
  insights: LogInsight[]
  errorCount: number
  warnCount: number
}

/** §39 — repair action catalogue. */
export type RepairAction =
  | 'instance_metadata'
  | 'missing_dirs'
  | 'option_files'
  | 'launcher_config'
  | 'downloads'
  | 'caches'
  | 'resource_packs'
  | string

export interface RepairFinding {
  [key: string]: unknown
}

export interface RepairPlannedStep {
  kind: 'mkdir' | 'write_json' | 'rewrite_lines' | string
  path?: string
  detail?: string
  [key: string]: unknown
}

export interface RepairScan {
  action: RepairAction
  findings: RepairFinding[]
  planned: RepairPlannedStep[]
  hasIssues: boolean
}

export interface RepairRunResult {
  done: string[]
  trashed: number
  [key: string]: unknown
}

/** Evidence export (on-request — JSON thuần, không secret). */
export interface DiagnosticExport {
  generatedAt: number
  appVersion: string
  instanceId: string | null
  sources: LogSourceInfo[]
  analysis: {
    insights: LogInsight[]
    errorCount: number
    warnCount: number
  }
  repairScans: RepairScan[]
}
