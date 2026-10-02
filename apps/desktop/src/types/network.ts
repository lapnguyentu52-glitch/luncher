/**
 * B11 — Network Lab (§42).
 * Mirror services/diagnostics/net.py + sidecar net.probe/net.dns payloads.
 */

/** TCP check result (mirror net.tcp_check). */
export interface TcpCheck {
  id?: string
  host: string
  port: number
  ok: boolean
  ms: number | null
  error: string | null
}

/** §42 — Minecraft Server List Ping result. */
export interface McPing {
  host: string
  port: number
  online: boolean
  motd?: string
  players?: { online: number; max: number }
  version?: { name: string; protocol: number }
  latencyMs?: number
  connectMs?: number
  favicon?: boolean
  modinfo?: Record<string, unknown> | null
  error?: string | null
}

/** RTT/jitter/loss stats từ probe (mục 42). */
export interface ProbeStats {
  min: number | null
  avg: number | null
  max: number | null
  jitter: number | null
  loss: number
}

/** 1 mẫu trong network timeline. */
export interface ProbeSample {
  seq: number
  ok: boolean
  ms: number | null
  error: string | null
  at: number
}

export interface NetProbe {
  host: string
  port: number
  count: number
  ok: number
  stats: ProbeStats
  timeline: ProbeSample[]
}

export interface DnsResult {
  host: string
  ok: boolean
  addresses: string[]
  ms?: number
  error?: string | null
}
