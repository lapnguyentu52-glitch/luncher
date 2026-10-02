import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import { checkEndpoints, dnsResolve, mcPing, netProbe, tcpCheck } from '@/services/networkCommands'

describe('network commands (B11 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'net.endpoints':
          return {
            ok: true,
            data: {
              endpoints: [
                { id: 'mojang', host: 'launchermeta.mojang.com', port: 443, ok: true, ms: 21.5, error: null },
              ],
            },
            warnings: [],
          }
        case 'net.tcp':
          return {
            ok: true,
            data: { tcp: { host: String(a.params?.host), port: Number(a.params?.port), ok: true, ms: 12.3, error: null } },
            warnings: [],
          }
        case 'net.ping':
          return {
            ok: true,
            data: {
              ping: {
                host: 'mc.hypixel.net',
                port: 25565,
                online: true,
                motd: 'Hypixel',
                players: { online: 100, max: 200 },
                version: { name: '1.21', protocol: 767 },
                latencyMs: 35.2,
                connectMs: 12.0,
                favicon: false,
                modinfo: null,
              },
            },
            warnings: [],
          }
        case 'net.probe':
          return {
            ok: true,
            data: {
              probe: {
                host: 'example.com',
                port: 443,
                count: 3,
                ok: 3,
                stats: { min: 10, avg: 12, max: 14, jitter: 2, loss: 0 },
                timeline: [
                  { seq: 0, ok: true, ms: 10, error: null, at: 0 },
                  { seq: 1, ok: true, ms: 12, error: null, at: 0.1 },
                  { seq: 2, ok: true, ms: 14, error: null, at: 0.2 },
                ],
              },
            },
            warnings: [],
          }
        case 'net.dns':
          return {
            ok: true,
            data: { dns: { host: String(a.params?.host), ok: true, addresses: ['93.184.216.34'], ms: 5.1 } },
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

  it('checkEndpoints trả endpoints', async () => {
    const endpoints = await checkEndpoints()
    expect(endpoints[0]?.id).toBe('mojang')
    expect(endpoints[0]?.ok).toBe(true)
  })

  it('tcpCheck trả RTT', async () => {
    const tcp = await tcpCheck('example.com', 443)
    expect(tcp.ok).toBe(true)
    expect(tcp.ms).toBeGreaterThan(0)
  })

  it('mcPing trả MOTD + players + latency', async () => {
    const ping = await mcPing('mc.hypixel.net')
    expect(ping.online).toBe(true)
    expect(ping.motd).toBe('Hypixel')
    expect(ping.players?.max).toBe(200)
    expect(ping.latencyMs).toBeGreaterThan(0)
  })

  it('netProbe trả stats + timeline', async () => {
    const probe = await netProbe('example.com', 443, 3)
    expect(probe.count).toBe(3)
    expect(probe.stats.jitter).toBe(2)
    expect(probe.timeline).toHaveLength(3)
    expect(probe.timeline[0]?.seq).toBe(0)
  })

  it('dnsResolve trả addresses', async () => {
    const dns = await dnsResolve('example.com')
    expect(dns.ok).toBe(true)
    expect(dns.addresses[0]).toBe('93.184.216.34')
  })
})
