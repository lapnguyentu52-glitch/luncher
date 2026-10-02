import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  clearPackets,
  exportPackets,
  getMetrics,
  getPacketStats,
  getRuntimeEndpoint,
  ingestPacket,
  listPackets,
  listSessions,
  setPacketCapture,
  startRuntime,
  writePairing,
} from '@/services/runtimeCommands'

describe('runtime commands (B12 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'runtime.endpoint':
          return {
            ok: true,
            data: { endpoint: { url: 'http://127.0.0.1:8971/packet', token: 'tok-123456789', packetTypes: ['runtime.hello'] } },
            warnings: [],
          }
        case 'runtime.start':
          return { ok: true, data: { started: { url: 'http://127.0.0.1:8971/packet' } }, warnings: [] }
        case 'runtime.sessions':
          return {
            ok: true,
            data: { sessions: [{ sessionId: 's1', client: '127.0.0.1', instanceId: 'i1', connectedAt: 1, lastSeen: 2 }] },
            warnings: [],
          }
        case 'runtime.metrics':
          return {
            ok: true,
            data: { metrics: { sessions: { s1: [{ ts: 1, fps: 120, frameMs: 8.3 }] } } },
            warnings: [],
          }
        case 'runtime.pairing':
          return { ok: true, data: { pairing: { file: '/x/companion.json' } }, warnings: [] }
        case 'packet.ingest':
          return { ok: true, data: { packetId: 1 }, warnings: [] }
        case 'packet.list':
          return {
            ok: true,
            data: {
              packets: [
                { timestamp: 1, direction: 'in', packetId: 1, name: 'runtime.performance', size: 64 },
              ],
              stats: { count: 1, cap: 10000, dropped: 0, bytes: 64, capturePayload: false },
            },
            warnings: [],
          }
        case 'packet.stats':
          return { ok: true, data: { stats: { count: 0, cap: 10000, dropped: 0, bytes: 0, capturePayload: false } }, warnings: [] }
        case 'packet.capture':
          return {
            ok: true,
            data: { capture: Boolean(a.params?.enabled), stats: { count: 0, cap: 10000, dropped: 0, bytes: 0, capturePayload: Boolean(a.params?.enabled) } },
            warnings: [],
          }
        case 'packet.clear':
          return { ok: true, data: { cleared: true, stats: { count: 0, cap: 10000, dropped: 0, bytes: 0, capturePayload: false } }, warnings: [] }
        case 'packet.export':
          return {
            ok: true,
            data: {
              export: {
                packets: [{ timestamp: 1, direction: 'in', packetId: 1, name: 'runtime.chat', size: 32 }],
                stats: { count: 1, cap: 10000, dropped: 0, bytes: 32, capturePayload: false },
              },
            },
            warnings: [],
          }
        default:
          return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method }, warnings: [] }
      }
    })
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('getRuntimeEndpoint trả url + token + packetTypes', async () => {
    const endpoint = await getRuntimeEndpoint()
    expect(endpoint.url).toContain('/packet')
    expect(endpoint.packetTypes).toContain('runtime.hello')
  })

  it('startRuntime trả url', async () => {
    const started = await startRuntime()
    expect(started.url).toContain('127.0.0.1')
  })

  it('listSessions trả sessions', async () => {
    const sessions = await listSessions()
    expect(sessions[0]?.sessionId).toBe('s1')
  })

  it('getMetrics trả fps history', async () => {
    const metrics = await getMetrics('s1')
    expect(metrics.sessions.s1?.[0]?.fps).toBe(120)
  })

  it('writePairing trả file', async () => {
    const pairing = await writePairing('i1')
    expect(pairing?.file.endsWith('companion.json')).toBe(true)
  })

  it('ingestPacket trả packetId', async () => {
    const id = await ingestPacket({ version: 1, type: 'runtime.chat', payload: { message: 'hi' } })
    expect(id).toBe(1)
  })

  it('listPackets trả packets + stats', async () => {
    const result = await listPackets()
    expect(result.packets[0]?.name).toBe('runtime.performance')
    expect(result.stats.cap).toBe(10000)
  })

  it('getPacketStats + setPacketCapture', async () => {
    const stats = await getPacketStats()
    expect(stats.count).toBe(0)
    const capture = await setPacketCapture(true)
    expect(capture.capture).toBe(true)
    expect(capture.stats.capturePayload).toBe(true)
  })

  it('clearPackets trả stats rỗng', async () => {
    const stats = await clearPackets()
    expect(stats.count).toBe(0)
  })

  it('exportPackets trả packets', async () => {
    const result = await exportPackets()
    expect(result.packets).toHaveLength(1)
    expect(result.packets[0]?.name).toBe('runtime.chat')
  })
})
