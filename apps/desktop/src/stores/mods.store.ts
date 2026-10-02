import { defineStore } from 'pinia'

import {
  autofixMods,
  cancelModpack,
  deleteQuarantined,
  getModDetail,
  getModsHealth,
  installMod,
  installMrpack,
  listInstalledMods,
  listQuarantine,
  pollModpackStatus,
  removeMod,
  restoreQuarantined,
  scanMods,
  searchMods,
} from '@/services/modsCommands'
import { toAntaresError } from '@/services/ipc'
import type {
  InstalledMod,
  ModDetail,
  ModsHealth,
  ModsScanResult,
  ModSearchHit,
  ModpackTask,
  QuarantineEntry,
} from '@/types/mods'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'
export type ModsTab = 'installed' | 'discover' | 'security'

function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

interface ModsState {
  phase: LoadPhase
  tab: ModsTab
  items: InstalledMod[]
  health: ModsHealth | null
  scan: ModsScanResult | null
  scanning: boolean
  fixing: boolean
  hits: ModSearchHit[]
  searching: boolean
  quarantine: QuarantineEntry[]
  busyFilename: string | null
  selectedDetail: ModDetail | null
  detailLoading: boolean
  modpackTask: ModpackTask | null
  modpackPolling: boolean
  errorMessage: string | null
  errorCode: string | null
}

/** §11 — Mods sections: Installed / Discover / Security. */
export const useModsStore = defineStore('mods', {
  state: (): ModsState => ({
    phase: 'idle',
    tab: 'installed',
    items: [],
    health: null,
    scan: null,
    scanning: false,
    fixing: false,
    hits: [],
    searching: false,
    quarantine: [],
    busyFilename: null,
    selectedDetail: null,
    detailLoading: false,
    modpackTask: null,
    modpackPolling: false,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    issueCount(state): number {
      return state.health?.issues.length ?? 0
    },
    dangerousCount(state): number {
      return state.scan?.dangerous ?? 0
    },
  },
  actions: {
    setTab(tab: ModsTab): void {
      this.tab = tab
    },
    async load(instanceId: string): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        this.items = await listInstalledMods(instanceId)
        this.phase = 'ready'
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async refreshHealth(instanceId: string, includeOutdated = false): Promise<void> {
      try {
        this.health = await getModsHealth(instanceId, includeOutdated)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async refreshQuarantine(): Promise<void> {
      try {
        this.quarantine = await listQuarantine()
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async search(query: string, loader: string, mcVersion: string): Promise<void> {
      this.searching = true
      try {
        this.hits = await searchMods({ query, loader, mcVersion })
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        this.hits = []
      } finally {
        this.searching = false
      }
    },
    async install(instanceId: string, projectId: string, loader: string, mcVersion: string): Promise<boolean> {
      try {
        await installMod({ projectId, instanceId, loader, mcVersion })
        await this.load(instanceId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async remove(instanceId: string, filename: string): Promise<boolean> {
      this.busyFilename = filename
      try {
        await removeMod(instanceId, filename)
        this.items = this.items.filter((m) => m.filename !== filename)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busyFilename = null
      }
    },
    /** §164 — scan + auto-quarantine DANGEROUS. Heuristic — không guarantee. */
    async runScan(instanceId: string): Promise<boolean> {
      this.scanning = true
      try {
        this.scan = await scanMods(instanceId)
        await this.load(instanceId)
        await this.refreshQuarantine()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.scanning = false
      }
    },
    async runAutofix(instanceId: string): Promise<boolean> {
      this.fixing = true
      try {
        await autofixMods(instanceId)
        await this.refreshHealth(instanceId)
        await this.load(instanceId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.fixing = false
      }
    },
    async restoreFromQuarantine(quarantineFile: string): Promise<boolean> {
      try {
        await restoreQuarantined(quarantineFile)
        await this.refreshQuarantine()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async deleteFromQuarantine(quarantineFile: string): Promise<boolean> {
      try {
        await deleteQuarantined(quarantineFile)
        await this.refreshQuarantine()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async selectDetail(instanceId: string, filename: string | null): Promise<void> {
      this.selectedDetail = null
      if (!filename) return
      this.detailLoading = true
      try {
        this.selectedDetail = await getModDetail(instanceId, filename)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.detailLoading = false
      }
    },
    /** B7b — install .mrpack async + poll 1s (§171 cadence, không spam). */
    async installModpack(mrpackPath: string, instanceId: string, optionalSelected?: string[]): Promise<boolean> {
      try {
        const params: { mrpackPath: string; instanceId: string; optionalSelected?: string[] } =
          { mrpackPath, instanceId }
        if (optionalSelected !== undefined) params.optionalSelected = optionalSelected
        const taskId = await installMrpack(params)
        this.modpackTask = {
          id: taskId, type: 'MODPACK_INSTALL', owner: `instance:${instanceId}`,
          state: 'running', progress: 0, message: 'Starting…',
        }
        void this.pollTask(taskId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async pollTask(taskId: string): Promise<void> {
      if (this.modpackPolling) return
      this.modpackPolling = true
      try {
        for (;;) {
          await new Promise((resolve) => setTimeout(resolve, 1000))
          const task = await pollModpackStatus(taskId)
          this.modpackTask = task
          if (task.state !== 'running' && task.state !== 'pending') break
        }
      } catch {
        // poll lỗi (sidecar restart…) — dừng im, UI giữ state cuối
      } finally {
        this.modpackPolling = false
      }
    },
    async cancelModpackTask(): Promise<boolean> {
      const taskId = this.modpackTask?.id
      if (!taskId) return false
      try {
        await cancelModpack(taskId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    clearModpackTask(): void {
      this.modpackTask = null
    },
  },
})
