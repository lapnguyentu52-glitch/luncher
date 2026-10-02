import { callLegacy } from './legacyCommands'
import type {
  ConsoleInsights,
  DiagnosticExport,
  LogAnalysis,
  LogReadResult,
  LogSourceInfo,
  RepairAction,
  RepairRunResult,
  RepairScan,
} from '@/types/diagnostics'

/**
 * B13 — Diagnostics qua legacy bridge (§41/§39/§16).
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

// ---- Log center (§41/§16) ----

export async function getLogSources(instanceId?: string): Promise<LogSourceInfo[]> {
  const params: { instanceId?: string } = {}
  if (instanceId) params.instanceId = instanceId
  const data = await callLegacy<{ sources: LogSourceInfo[] }>('console.sources', params)
  return data.sources
}

/** §16 — đọc 1 nguồn → [{n, text}] (limit cap 20000 phía sidecar). */
export async function readLog(source: string, instanceId?: string, limit = 5000): Promise<LogReadResult> {
  const params: { source: string; instanceId?: string; limit: number } = { source, limit }
  if (instanceId) params.instanceId = instanceId
  const data = await callLegacy<{ log: LogReadResult }>('console.read', params)
  return data.log
}

/** §41 — blueprint analysis (không LLM): insights + error/warn counts. */
export async function analyzeLogs(instanceId?: string, source?: string): Promise<LogAnalysis> {
  const params: { instanceId?: string; source?: string } = {}
  if (instanceId) params.instanceId = instanceId
  if (source) params.source = source
  const data = await callLegacy<{ analysis: LogAnalysis }>('console.analyze', params)
  return data.analysis
}

export async function getInsights(instanceId?: string): Promise<ConsoleInsights> {
  const params: { instanceId?: string } = {}
  if (instanceId) params.instanceId = instanceId
  return callLegacy<ConsoleInsights>('console.insights', params)
}

// ---- Repair (§39) ----

export async function getRepairActions(): Promise<RepairAction[]> {
  const data = await callLegacy<{ actions: RepairAction[] }>('repair.actions')
  return data.actions
}

/** Dry-run "What will change?" — findings + planned, không ghi. */
export async function scanRepair(action: RepairAction, instanceId?: string): Promise<RepairScan> {
  const params: { action: string; instanceId?: string } = { action }
  if (instanceId) params.instanceId = instanceId
  const data = await callLegacy<{ scan: RepairScan }>('repair.scan', params)
  return data.scan
}

export async function runRepair(action: RepairAction, instanceId?: string): Promise<RepairRunResult> {
  const params: { action: string; instanceId?: string } = { action }
  if (instanceId) params.instanceId = instanceId
  const data = await callLegacy<{ result: RepairRunResult }>('repair.run', params)
  return data.result
}

/** Evidence export — gói JSON on-request. */
export async function exportDiagnostics(instanceId?: string): Promise<DiagnosticExport> {
  const params: { instanceId?: string } = {}
  if (instanceId) params.instanceId = instanceId
  const data = await callLegacy<{ export: DiagnosticExport }>('diagnostic.export', params)
  return data.export
}
