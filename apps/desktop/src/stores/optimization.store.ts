import { defineStore } from 'pinia'

import {
  applyOptimization,
  cleanPaths,
  emptyTrash,
  getOptimizationSnapshot,
  getPowerStatus,
  getSystemOverview,
  planOptimization,
  rollbackOptimization,
  scanCleanup,
  scanOptimization,
  setPowerPlan,
  undoCleanup,
} from '@/services/optimizationCommands'
import { toAntaresError } from '@/services/ipc'
import type {
  CleanupPreview,
  OptimizationApplyResult,
  OptimizationPlan,
  OptimizationScan,
  OptimizationSnapshotInfo,
  PowerStatus,
  SystemOverview,
} from '@/types/optimization'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'
export type OptTab = 'game' | 'system'

function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

interface OptimizationState {
  phase: LoadPhase
  tab: OptTab
  /** Game optimization (§3/§70). */
  scan: OptimizationScan | null
  plan: OptimizationPlan | null
  planning: boolean
  applying: boolean
  selectedProfileId: string
  snapshot: OptimizationSnapshotInfo | null
  lastApply: OptimizationApplyResult | null
  /** System optimization (§4/§35–§37). */
  overview: SystemOverview | null
  cleanup: CleanupPreview | null
  selectedCleanupPaths: string[]
  lastCleanId: string | null
  power: PowerStatus | null
  busy: boolean
  errorMessage: string | null
  errorCode: string | null
}

/**
 * B10 — Optimization store: game (plan-first §3.1, snapshot/rollback §70)
 * + system (overview/cleanup undo-able §37/power §36). Phase machine §158.
 */
export const useOptimizationStore = defineStore('optimization', {
  state: (): OptimizationState => ({
    phase: 'idle',
    tab: 'game',
    scan: null,
    plan: null,
    planning: false,
    applying: false,
    selectedProfileId: '',
    snapshot: null,
    lastApply: null,
    overview: null,
    cleanup: null,
    selectedCleanupPaths: [],
    lastCleanId: null,
    power: null,
    busy: false,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    selectedProfile(state): { id: string; labelKey: string } | undefined {
      return state.scan?.profiles.find((p) => p.id === state.selectedProfileId)
    },
  },
  actions: {
    setTab(tab: OptTab): void {
      this.tab = tab
    },
    async load(instanceId?: string): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        this.scan = await scanOptimization(instanceId)
        this.selectedProfileId = ''
        this.plan = null
        this.phase = 'ready'
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    /** §3.1 — plan-first: luôn preview diff trước khi apply. */
    async loadPlan(instanceId: string, profileId: string): Promise<boolean> {
      this.planning = true
      this.selectedProfileId = profileId
      try {
        this.plan = await planOptimization(instanceId, profileId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        this.plan = null
        return false
      } finally {
        this.planning = false
      }
    },
    async apply(instanceId: string): Promise<boolean> {
      if (!this.plan || !this.plan.hasChanges) return false
      this.applying = true
      try {
        this.lastApply = await applyOptimization(instanceId, this.plan.profileId)
        this.snapshot = await getOptimizationSnapshot(instanceId)
        this.plan = null
        await this.load(instanceId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.applying = false
      }
    },
    /** §70 — rollback 1 click từ snapshot gần nhất. */
    async rollback(instanceId: string): Promise<boolean> {
      this.busy = true
      try {
        await rollbackOptimization(instanceId)
        this.snapshot = null
        this.lastApply = null
        await this.load(instanceId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busy = false
      }
    },
    async refreshSnapshot(instanceId: string): Promise<void> {
      try {
        this.snapshot = await getOptimizationSnapshot(instanceId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    // ---- System ----
    async loadSystem(): Promise<void> {
      this.busy = true
      try {
        this.overview = await getSystemOverview()
        this.cleanup = this.overview.cleanupPreview
        this.power = this.overview.power
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.busy = false
      }
    },
    async rescanCleanup(): Promise<void> {
      try {
        this.cleanup = await scanCleanup()
        this.selectedCleanupPaths = []
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    toggleCleanupPath(path: string): void {
      if (this.selectedCleanupPaths.includes(path)) {
        this.selectedCleanupPaths = this.selectedCleanupPaths.filter((p) => p !== path)
      } else {
        this.selectedCleanupPaths.push(path)
      }
    },
    /** §37 — clean undo-able: giữ cleanId để undo. */
    async clean(): Promise<boolean> {
      if (this.selectedCleanupPaths.length === 0) return false
      this.busy = true
      try {
        const result = await cleanPaths(this.selectedCleanupPaths)
        this.lastCleanId = result.cleanId ?? null
        this.cleanup = await scanCleanup()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busy = false
      }
    },
    async undoClean(): Promise<boolean> {
      if (!this.lastCleanId) return false
      this.busy = true
      try {
        await undoCleanup(this.lastCleanId)
        this.lastCleanId = null
        this.cleanup = await scanCleanup()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busy = false
      }
    },
    async emptyTrash(): Promise<number | null> {
      this.busy = true
      try {
        const removed = await emptyTrash()
        this.cleanup = await scanCleanup()
        return removed
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return null
      } finally {
        this.busy = false
      }
    },
    async refreshPower(): Promise<void> {
      try {
        this.power = await getPowerStatus()
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async switchPowerPlan(planId: string): Promise<boolean> {
      this.busy = true
      try {
        await setPowerPlan(planId)
        await this.refreshPower()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busy = false
      }
    },
  },
})
