import { defineStore } from 'pinia'

import { checkEndpoints, dnsResolve, mcPing, netProbe, tcpCheck } from '@/services/networkCommands'
import { toAntaresError } from '@/services/ipc'
import type { DnsResult, McPing, NetProbe, TcpCheck } from '@/types/network'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'

function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

interface NetworkState {
  phase: LoadPhase
  endpoints: TcpCheck[]
  ping: McPing | null
  pinging: boolean
  probe: NetProbe | null
  probing: boolean
  tcp: TcpCheck | null
  dns: DnsResult | null
  targetHost: string
  targetPort: number
  errorMessage: string | null
  errorCode: string | null
}

/**
 * B11 — Network Lab store (§42): endpoints overview + MC server ping +
 * RTT/jitter probe timeline + DNS resolve. Phase machine §158.
 */
export const useNetworkStore = defineStore('network', {
  state: (): NetworkState => ({
    phase: 'idle',
    endpoints: [],
    ping: null,
    pinging: false,
    probe: null,
    probing: false,
    tcp: null,
    dns: null,
    targetHost: '',
    targetPort: 25565,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    unreachableCount(state): number {
      return state.endpoints.filter((e) => !e.ok).length
    },
    lastProbeStats(state): NetProbe['stats'] | null {
      return state.probe?.stats ?? null
    },
  },
  actions: {
    async load(): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        this.endpoints = await checkEndpoints()
        this.phase = 'ready'
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    /** §42 — ping MC server: MOTD/players/latency. */
    async runPing(): Promise<boolean> {
      if (!this.targetHost.trim()) return false
      this.pinging = true
      try {
        this.ping = await mcPing(this.targetHost.trim(), this.targetPort)
        return this.ping.online
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.pinging = false
      }
    },
    /** §42 — RTT/jitter probe; UI gọi khi bấm nút (không poll — §171). */
    async runProbe(count = 10): Promise<boolean> {
      if (!this.targetHost.trim()) return false
      this.probing = true
      try {
        this.probe = await netProbe(this.targetHost.trim(), this.targetPort, count)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.probing = false
      }
    },
    async runTcp(): Promise<boolean> {
      if (!this.targetHost.trim()) return false
      try {
        this.tcp = await tcpCheck(this.targetHost.trim(), this.targetPort)
        return this.tcp.ok
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async runDns(): Promise<boolean> {
      if (!this.targetHost.trim()) return false
      try {
        this.dns = await dnsResolve(this.targetHost.trim())
        return this.dns.ok
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
  },
})
