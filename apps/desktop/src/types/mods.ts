/**
 * B7/M7 — Mod Manager (§11/§162–§164).
 * Mirror services/mods/service.py + services/mods/security.py payloads.
 */

/** §11 — Mod card metadata (từ health_check, best-effort ở mods.list). */
export interface InstalledMod {
  filename: string
  modId?: string
  name?: string
  version?: string
  loader?: string
  readable?: boolean
}

/** Modrinth search hit (mirror api.py mods_search shape). */
export interface ModSearchHit {
  projectId: string
  title: string
  author?: string
  downloads?: number
}

/** §162 — health issue: missing dep / wrong loader / not readable. */
export interface ModIssue {
  mod: string
  kind: string
  detail: string
  fixable: boolean
  suggestion?: string
}

export interface ModOutdated {
  mod: string
  detail: string
  suggestion?: string
}

export interface ModsHealth {
  mods: Array<{
    filename: string
    modId?: string
    name?: string
    version?: string
    loader?: string
    depends?: string[]
    readable?: boolean
  }>
  issues: ModIssue[]
  outdated: ModOutdated[]
}

/** §164 — scan verdicts: SAFE / SUSPICIOUS / DANGEROUS (heuristic, không phải guarantee). */
export type ScanVerdict = 'SAFE' | 'SUSPICIOUS' | 'DANGEROUS' | string

export interface ScanFinding {
  ruleId: string
  severity: string
  weight?: number
  title: string
  detail: string
  evidence?: string[]
}

export interface ScanReport {
  file: string
  size: number
  sha256: string
  verdict: ScanVerdict
  score: number
  classesScanned?: number
  javaVersion?: number
  modId?: string | null
  modName?: string | null
  findings: ScanFinding[]
  quarantined?: boolean
  quarantineError?: string
}

export interface ModsScanResult {
  results: ScanReport[]
  dangerous: number
  suspicious: number
  safe: number
}

export interface ModsFixResult {
  downloaded?: string[]
  failed?: string[]
  [key: string]: unknown
}

/** Quarantine entry (mirror services/mods/scanner/quarantine.py). */
export interface QuarantineEntry {
  quarantineFile: string
  originalName: string
  originalPath: string
  instanceId?: string
  verdict: string
  score: number
  findings: Array<{ ruleId: string; title: string; severity: string }>
  quarantinedAt: number
}

/** §11 — mod detail: metadata + dependency graph fields (read-only). */
export interface ModDetail {
  filename: string
  modId?: string
  name?: string
  version?: string
  loader?: string
  mcVersions: string[]
  depends: Record<string, string>
  recommends: Record<string, string>
  breaks: Record<string, string>
  fabricModJson: boolean
  forgeToml: boolean
  readable: boolean
}

/** B7b — .mrpack preview (mirror read_mrpack_info). */
export interface MrpackInfo {
  name: string
  summary?: string
  versionId?: string
  minecraftVersion: string
  loader: string
  loaderVersion?: string | null
  optionalFiles: string[]
}

/** Task state mirror (§90) — modpack install chạy async. */
export interface ModpackTask {
  id: string
  type: string
  owner: string
  state: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled' | string
  progress: number
  message: string
  error?: { code: string; message: string } | null
  result?: { name?: string; minecraftVersion?: string; loader?: string; [key: string]: unknown } | null
}
