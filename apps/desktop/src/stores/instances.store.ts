import { defineStore } from 'pinia'

import { listInstances, runPreflight, selectInstance } from '@/services/flowsCommands'
import { toAntaresError } from '@/services/ipc'
import type { AntaresInstance, PreflightResult } from '@/types/instances'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'

/** §158 — mỗi feature có đủ state machine loading/empty/error. */
interface InstancesState {
  phase: LoadPhase
  items: AntaresInstance[]
  selectedId: string | null
  preflight: PreflightResult | null
  preflightLoading: boolean
  errorMessage: string | null
  errorCode: string | null
}

export const useInstancesStore = defineStore('instances', {
  state: (): InstancesState => ({
    phase: 'idle',
    items: [],
    selectedId: null,
    preflight: null,
    preflightLoading: false,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    selected(state): AntaresInstance | undefined {
      return state.items.find((i) => i.id === state.selectedId)
    },
  },
  actions: {
    async load(): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        this.items = await listInstances()
        this.phase = 'ready'
      } catch (err) {
        const antares = toAntaresError(err)
        // bridge chưa chạy / không có sidecar → offline mode (§68)
        this.phase = antares.code === 'IPC_SESSION_STALE' || antares.code === 'IPC_SIDECAR_NOT_FOUND' ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async select(instanceId: string): Promise<void> {
      this.selectedId = instanceId
      this.preflight = null
      try {
        await selectInstance(instanceId)
      } catch {
        // selection persist lỗi không chặn UI — selectedId đã set local
      }
    },
    async refreshPreflight(): Promise<void> {
      if (!this.selectedId) return
      this.preflightLoading = true
      try {
        this.preflight = await runPreflight(this.selectedId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.preflight = {
          instanceId: this.selectedId,
          checks: [{ id: 'preflight', label: 'Preflight', status: 'error', detail: antares.message }],
          blockers: 1,
          warnings: 0,
          canPlay: false,
        }
      } finally {
        this.preflightLoading = false
      }
    },
  },
})
