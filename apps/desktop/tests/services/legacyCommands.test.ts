import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  callLegacy,
  getLegacyStatus,
  startLegacy,
} from '@/services/legacyCommands'
import { LEGACY_PROTOCOL_VERSION } from '@/types/legacy'
import type { LegacyVersionInfo } from '@/types/legacy'

const versionInfo: LegacyVersionInfo = {
  protocol: LEGACY_PROTOCOL_VERSION,
  service: 'antares-legacy',
  serviceVersion: '4.0.0',
  python: '3.14.2',
}

describe('legacy bridge commands (M5 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    let ready = false
    registerBrowserFallback('legacy_start', () => {
      ready = true
      return { ok: true, data: versionInfo, warnings: [] }
    })
    registerBrowserFallback('legacy_status', () => ({
      ok: true,
      data: { running: ready, program: 'legacy/python/sidecar.py', version: ready ? versionInfo : undefined, crashCount: 0 },
      warnings: [],
    }))
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: unknown }
      if (!ready) {
        return { ok: false, error: { code: 'IPC_SESSION_STALE', message: 'bridge not running', retryable: true }, warnings: [] }
      }
      if (a.method === 'app.echo') return { ok: true, data: a.params, warnings: [] }
      return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method, retryable: false }, warnings: [] }
    })
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('start → handshake info với protocol khớp', async () => {
    const info = await startLegacy()
    expect(info.protocol).toBe(LEGACY_PROTOCOL_VERSION)
    expect(info.service).toBe('antares-legacy')
  })

  it('status phản ánh running sau start', async () => {
    const before = await getLegacyStatus()
    expect(before.running).toBe(false)
    await startLegacy()
    const after = await getLegacyStatus()
    expect(after.running).toBe(true)
    expect(after.program).toContain('sidecar.py')
  })

  it('call echo roundtrip params', async () => {
    await startLegacy()
    const echoed = await callLegacy<{ x: number }>('app.echo', { x: 42 })
    expect(echoed.x).toBe(42)
  })

  it('call trước start → IPC_SESSION_STALE (retryable)', async () => {
    await expect(callLegacy('app.echo', {})).rejects.toMatchObject({
      antares: { code: 'IPC_SESSION_STALE', retryable: true },
    })
  })

  it('method không tồn tại → METHOD_NOT_FOUND', async () => {
    await startLegacy()
    await expect(callLegacy('no.such')).rejects.toMatchObject({
      antares: { code: 'METHOD_NOT_FOUND' },
    })
  })
})
