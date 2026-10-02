import { defineStore } from 'pinia'

import {
  analyzeLogs,
  exportDiagnostics,
  getInsights,
  getLogSources,
  getRepairActions,
  readLog,
  runRepair,
  scanRepair,
} from '@/services/diagnosticsCommands'
import { toAntaresError } from '@/services/ipc'
import type {
  ConsoleInsights,
  DiagnosticExport,
  LogAnalysis,
  LogReadResult,
  LogSourceInfo,
  RepairAction,
  RepairScan,
} from '@/types/diagnostics'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'

function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

interface DiagnosticsState {
  phase: LoadPhase
  sources: LogSourceInfo[]
  log: LogReadResult | null
  logLoading: boolean
  selectedSource: string
  insights: ConsoleInsights | null
  analyzing: boolean
  // Repair
  actions: RepairAction[]
  selectedAction: string
  scan: RepairScan | null
  scanning: boolean
  running: boolean
  runSummary: string[] | null
  exporting: boolean
  lastExport: DiagnosticExport | null
  errorMessage: string | null
  errorCode: string | null
}

/**
 * B13 — Diagnostics store: log center (§41/§16) + blueprint insights +
 * repair scan-first (§39 — dry-run trước run) + evidence export.
 */
export const useDiagnosticsStore = defineStore('diagnostics', {
  state: (): DiagnosticsState => ({
    phase: 'idle',
    sources: [],
    log: null,
    logLoading: false,
    selectedSource: 'launcher',
    insights: null,
    analyzing: false,
    actions: [],
    selectedAction: '',
    scan: null,
    scanning: false,
    running: false,
    runSummary: null,
    exporting: false,
    lastExport: null,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    issueCount(state): number {
      return (state.insights?.errorCount ?? 0) + (state.insights?.warnCount ?? 0)
    },
  },
  actions: {
    async load(instanceId?: string): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        const [sources, insights, actions] = await Promise.all([
          getLogSources(instanceId),
          getInsights(instanceId),
          getRepairActions(),
        ])
        this.sources = sources
        this.insights = insights
        this.actions = actions
        this.phase = 'ready'
        void this.read(this.selectedSource, instanceId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async read(source: string, instanceId?: string): Promise<void> {
      this.selectedSource = source
      this.logLoading = true
      try {
        this.log = await readLog(source, instanceId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.logLoading = false
      }
    },
    async reanalyze(instanceId?: string): Promise<boolean> {
      this.analyzing = true
      try {
        this.insights = await getInsights(instanceId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.analyzing = false
      }
    },
    /** Full analysis (kèm sources/lines) khi cần chi tiết. */
    async fullAnalysis(instanceId?: string): Promise<LogAnalysis | null> {
      try {
        return await analyzeLogs(instanceId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return null
      }
    },
    /** §39 — scan-first: luôn dry-run trước run. */
    async loadScan(action: RepairAction, instanceId?: string): Promise<boolean> {
      this.scanning = true
      this.selectedAction = action
      this.runSummary = null
      try {
        this.scan = await scanRepair(action, instanceId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        this.scan = null
        return false
      } finally {
        this.scanning = false
      }
    },
    async run(action: RepairAction, instanceId?: string): Promise<boolean> {
      this.running = true
      try {
        const result = await runRepair(action, instanceId)
        this.runSummary = result.done ?? []
        this.scan = null
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.running = false
      }
    },
    /** Evidence export — gói JSON download. */
    async export(instanceId?: string): Promise<boolean> {
      this.exporting = true
      try {
        this.lastExport = await exportDiagnostics(instanceId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.exporting = false
      }
    },
  },
})
