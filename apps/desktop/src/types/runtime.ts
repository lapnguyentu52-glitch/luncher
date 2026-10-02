/**
 * B12 — Runtime companion + Packet inspector (§12/§13/§121/§122).
 * Mirror services/runtime/service.py + sidecar _PacketRing payloads.
 */

/** §12 — companion session (mirror RuntimeService._sessions). */
export interface RuntimeSession {
  sessionId: string
  client: string
  instanceId?: string
  connectedAt: number
  lastSeen: number
}

/** 1 điểm metric FPS (ring RAM ~600 điểm/session — mục 62). */
export interface RuntimeMetricPoint {
  ts: number
  fps?: number
  frameMs?: number
  low1?: number
}

export interface RuntimeMetrics {
  sessions: Record<string, RuntimeMetricPoint[]>
}

/** §12 — IPC endpoint cho companion setup (token xoay mỗi start). */
export interface RuntimeEndpoint {
  url: string
  token: string
  packetTypes: string[]
}

/** §121 — packet metadata entry (payload chỉ có khi capture bật). */
export interface PacketEntry {
  timestamp: number
  direction: 'in' | 'out' | string
  packetId: number
  name: string
  size: number
  payload?: Record<string, unknown>
}

export interface PacketStats {
  count: number
  cap: number
  dropped: number
  bytes: number
  capturePayload: boolean
}
