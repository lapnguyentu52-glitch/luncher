import { callLegacy } from './legacyCommands'
import type { DnsResult, McPing, NetProbe, TcpCheck } from '@/types/network'

/**
 * B11 — Network Lab qua legacy bridge (§42).
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

/** TCP check song song các endpoint launcher cần (mojang/modrinth/…). */
export async function checkEndpoints(): Promise<TcpCheck[]> {
  const data = await callLegacy<{ endpoints: TcpCheck[] }>('net.endpoints')
  return data.endpoints
}

/** 1 TCP connect + RTT cho host:port tuỳ ý. */
export async function tcpCheck(host: string, port: number, timeout?: number): Promise<TcpCheck> {
  const params: { host: string; port: number; timeout?: number } = { host, port }
  if (timeout !== undefined) params.timeout = timeout
  const data = await callLegacy<{ tcp: TcpCheck }>('net.tcp', params)
  return data.tcp
}

/** §42 — Minecraft Server List Ping: MOTD/players/version/latency. */
export async function mcPing(host: string, port = 25565, timeout?: number): Promise<McPing> {
  const params: { host: string; port: number; timeout?: number } = { host, port }
  if (timeout !== undefined) params.timeout = timeout
  const data = await callLegacy<{ ping: McPing }>('net.ping', params)
  return data.ping
}

/** RTT/jitter/loss + network timeline — N lần TCP connect (cap 30). */
export async function netProbe(host: string, port: number, count = 10, timeout?: number): Promise<NetProbe> {
  const params: { host: string; port: number; count: number; timeout?: number } = {
    host,
    port,
    count,
  }
  if (timeout !== undefined) params.timeout = timeout
  const data = await callLegacy<{ probe: NetProbe }>('net.probe', params)
  return data.probe
}

/** DNS resolve — IPv4/IPv6 + thời gian. */
export async function dnsResolve(host: string): Promise<DnsResult> {
  const data = await callLegacy<{ dns: DnsResult }>('net.dns', { host })
  return data.dns
}
