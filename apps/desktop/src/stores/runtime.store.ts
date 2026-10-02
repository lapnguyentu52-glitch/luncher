import { defineStore } from 'pinia'

import {
  clearPackets,
  exportPackets,
  getMetrics,
  getRuntimeEndpoint,
  listPackets,
  listSessions,
  setPacketCapture,
  startRuntime,
  writePairing,
} from '@/services/runtimeCommands'
import { toAntaresError } from '@/services/ipc'
import type {
  PacketEntry,
  PacketStats,
  RuntimeEndpoint,
  RuntimeSession,
} from '@/types/runtime'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'

function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

interface RuntimeState {
  phase: LoadPhase
  endpoint: RuntimeEndpoint | null
  sessions: RuntimeSession[]
  metrics: Record<string, Array<{ ts: number; fps?: number; frameMs?: number; low1?: number }>>
  metricsLoading: boolean
  pairingFile: string | null
  starting: boolean
  // Packet inspector (§121/§122)
  packets: PacketEntry[]
  packetStats: PacketStats | null
  packetsLoading: boolean
  exporting: boolean
  errorMessage: string | null
  errorCode: string | null
}

/**
 * B12 — Runtime store: companion sessions + FPS metrics (poll cadence §171)
 * + packet inspector ring buffer (metadata mode §121, memory-only §122).
 */
export const useRuntimeStore = defineStore('runtime', {
  state: (): RuntimeState => ({
    phase: 'idle',
    endpoint: null,
    sessions: [],
    metrics: {},
    metricsLoading: false,
    pairingFile: null,
    starting: false,
    packets: [],
    packetStats: null,
    packetsLoading: false,
    exporting: false,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    /** FPS timeline của session đầu tiên (metrics chart). */
    primaryFpsSeries(state): Array<{ ts: number; fps?: number; frameMs?: number; low1?: number }> {
      const first = Object.keys(state.metrics)[0]
      return first ? state.metrics[first] ?? [] : []
    },
  },
  actions: {
    async load(): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        const [endpoint, sessions, packetList] = await Promise.all([
          getRuntimeEndpoint(),
          listSessions(),
          listPackets(200),
        ])
        this.endpoint = endpoint
        this.sessions = sessions
        this.packets = packetList.packets
        this.packetStats = packetList.stats
        this.phase = 'ready'
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async start(): Promise<boolean> {
      this.starting = true
      try {
        await startRuntime()
        this.endpoint = await getRuntimeEndpoint()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.starting = false
      }
    },
    async pair(instanceId: string): Promise<boolean> {
      try {
        const result = await writePairing(instanceId)
        this.pairingFile = result?.file ?? null
        return this.pairingFile !== null
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    /** §171 — poll metrics cadence 2s (companion đẩy 2–4 packet/s). */
    async refreshMetrics(): Promise<void> {
      this.metricsLoading = true
      try {
        const metrics = await getMetrics()
        this.metrics = metrics.sessions
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.metricsLoading = false
      }
    },
    async refreshSessions(): Promise<void> {
      try {
        this.sessions = await listSessions()
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async refreshPackets(limit = 200): Promise<void> {
      this.packetsLoading = true
      try {
        const result = await listPackets(limit)
        this.packets = result.packets
        this.packetStats = result.stats
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.packetsLoading = false
      }
    },
    /** §121 — DEBUG PAYLOAD manual toggle (memory bounded). */
    async toggleCapture(enabled: boolean): Promise<boolean> {
      try {
        const result = await setPacketCapture(enabled)
        this.packetStats = result.stats
        await this.refreshPackets()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async clear(): Promise<boolean> {
      try {
        this.packetStats = await clearPackets()
        this.packets = []
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    /** §122 — export chỉ khi user bấm. */
    async exportPackets(): Promise<PacketEntry[] | null> {
      this.exporting = true
      try {
        const result = await exportPackets()
        return result.packets
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return null
      } finally {
        this.exporting = false
      }
    },
  },
})
