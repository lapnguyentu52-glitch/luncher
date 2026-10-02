import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  analyzeLogs,
  exportDiagnostics,
  getInsights,
  getLogSources,
  getRepairActions,
  readLog,
  runRepair,
  scanRepair,
} from '@/services/diagnosticsCommands'

describe('diagnostics commands (B13 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'console.sources':
          return {
            ok: true,
            data: {
              sources: [
                { id: 'launcher', available: true, bytes: 0, lines: 42 },
                { id: 'minecraft', available: false, bytes: 0, lines: 0 },
              ],
            },
            warnings: [],
          }
        case 'console.read':
          return {
            ok: true,
            data: {
              log: {
                source: 'launcher',
                instanceId: null,
                lines: [{ n: 1, text: 'INFO ready' }],
                truncated: false,
              },
            },
            warnings: [],
          }
        case 'console.analyze':
          return {
            ok: true,
            data: {
              analysis: {
                instanceId: null,
                sources: { launcher: 42 },
                insights: [],
                errorCount: 0,
                warnCount: 1,
                durationMs: 3,
              },
            },
            warnings: [],
          }
        case 'console.insights':
          return {
            ok: true,
            data: {
              insights: [{
                id: 'mod_conflict',
                count: 1,
                severity: 'error',
                seed: 'modConflict',
                recommend: 'nav.mods',
                action: { kind: 'route', target: 'mods' },
                firstSeen: null,
                lastSeen: null,
                sources: ['launcher'],
                lines: [{ source: 'launcher', line: 10, text: 'DuplicateModsFoundException' }],
              }],
              errorCount: 1,
              warnCount: 0,
            },
            warnings: [],
          }
        case 'repair.actions':
          return { ok: true, data: { actions: ['instance_metadata', 'missing_dirs'] }, warnings: [] }
        case 'repair.scan':
          return {
            ok: true,
            data: {
              scan: {
                action: String(a.params?.action),
                findings: [],
                planned: [{ kind: 'mkdir', path: '/x/mods', detail: 'created mods/' }],
                hasIssues: true,
              },
            },
            warnings: [],
          }
        case 'repair.run':
          return { ok: true, data: { result: { done: ['created mods/'], trashed: 0 } }, warnings: [] }
        case 'diagnostic.export':
          return {
            ok: true,
            data: {
              export: {
                generatedAt: 1,
                appVersion: '4.0.0',
                instanceId: null,
                sources: [],
                analysis: { insights: [], errorCount: 0, warnCount: 0 },
                repairScans: [],
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

  it('getLogSources trả sources', async () => {
    const sources = await getLogSources()
    expect(sources[0]?.id).toBe('launcher')
    expect(sources[0]?.available).toBe(true)
  })

  it('readLog trả lines', async () => {
    const log = await readLog('launcher', undefined, 100)
    expect(log.lines[0]?.text).toBe('INFO ready')
    expect(log.truncated).toBe(false)
  })

  it('analyzeLogs trả analysis', async () => {
    const analysis = await analyzeLogs()
    expect(analysis.warnCount).toBe(1)
    expect(analysis.durationMs).toBeGreaterThanOrEqual(0)
  })

  it('getInsights trả insights gọn', async () => {
    const insights = await getInsights()
    expect(insights.errorCount).toBe(1)
    expect(insights.insights[0]?.action.target).toBe('mods')
  })

  it('getRepairActions trả catalogue', async () => {
    const actions = await getRepairActions()
    expect(actions).toContain('missing_dirs')
  })

  it('scanRepair trả planned steps', async () => {
    const scan = await scanRepair('missing_dirs', 'i1')
    expect(scan.hasIssues).toBe(true)
    expect(scan.planned[0]?.kind).toBe('mkdir')
  })

  it('runRepair trả summary', async () => {
    const result = await runRepair('missing_dirs', 'i1')
    expect(result.done).toContain('created mods/')
  })

  it('exportDiagnostics trả evidence gói', async () => {
    const exp = await exportDiagnostics()
    expect(exp.appVersion).toBe('4.0.0')
    expect(exp.analysis).toBeDefined()
    expect(exp.repairScans).toEqual([])
  })
})
