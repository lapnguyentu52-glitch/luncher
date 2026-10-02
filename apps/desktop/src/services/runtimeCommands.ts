import { callLegacy } from './legacyCommands'
import type { PacketEntry, PacketStats, RuntimeEndpoint, RuntimeMetrics, RuntimeSession } from '@/types/runtime'

/**
 * B12 — Runtime companion + Packet inspector qua legacy bridge.
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

export async function getRuntimeEndpoint(): Promise<RuntimeEndpoint> {
  const data = await callLegacy<{ endpoint: RuntimeEndpoint }>('runtime.endpoint')
  return data.endpoint
}

export async function startRuntime(): Promise<{ url: string }> {
  const data = await callLegacy<{ started: { url: string } }>('runtime.start')
  return data.started
}

export async function listSessions(): Promise<RuntimeSession[]> {
  const data = await callLegacy<{ sessions: RuntimeSession[] }>('runtime.sessions')
  return data.sessions
}

/** FPS/frameMs history — ring RAM phía service (§12). */
export async function getMetrics(sessionId?: string): Promise<RuntimeMetrics> {
  const params: { sessionId?: string } = {}
  if (sessionId) params.sessionId = sessionId
  const data = await callLegacy<{ metrics: RuntimeMetrics }>('runtime.metrics', params)
  return data.metrics
}

/** Auto-pairing: ghi companion.json vào game dir (mục 12). */
export async function writePairing(instanceId: string): Promise<{ file: string } | null> {
  const data = await callLegacy<{ pairing: { file: string } | null }>('runtime.pairing', {
    instanceId,
  })
  return data.pairing
}

/** Nhận 1 packet từ companion (schema v1) — ghi metadata + đẩy service. */
export async function ingestPacket(packet: {
  version: 1
  type: string
  timestamp?: number
  payload: Record<string, unknown>
}, direction?: 'in' | 'out'): Promise<number> {
  const params: {
    packet: Record<string, unknown>
    direction?: 'in' | 'out'
  } = { packet }
  if (direction !== undefined) params.direction = direction
  const data = await callLegacy<{ packetId: number }>('packet.ingest', params)
  return data.packetId
}

export interface PacketListResult {
  packets: PacketEntry[]
  stats: PacketStats
}

export async function listPackets(limit = 200): Promise<PacketListResult> {
  return callLegacy<PacketListResult>('packet.list', { limit })
}

export async function getPacketStats(): Promise<PacketStats> {
  const data = await callLegacy<{ stats: PacketStats }>('packet.stats')
  return data.stats
}

/** §121 — DEBUG PAYLOAD mode manual on/off; cap tùy chọn (max 100k §122). */
export async function setPacketCapture(enabled: boolean, cap?: number): Promise<{ capture: boolean; stats: PacketStats }> {
  const params: { enabled: boolean; cap?: number } = { enabled }
  if (cap !== undefined) params.cap = cap
  return callLegacy<{ capture: boolean; stats: PacketStats }>('packet.capture', params)
}

export async function clearPackets(): Promise<PacketStats> {
  const data = await callLegacy<{ cleared: boolean; stats: PacketStats }>('packet.clear')
  return data.stats
}

/** §122 — export chỉ khi user yêu cầu. */
export async function exportPackets(): Promise<PacketListResult> {
  const data = await callLegacy<{ export: PacketListResult }>('packet.export')
  return data.export
}
