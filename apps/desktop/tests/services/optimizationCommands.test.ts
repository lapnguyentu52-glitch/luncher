import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  applyOptimization,
  cleanPaths,
  emptyTrash,
  getOptimizationSnapshot,
  getPowerStatus,
  getSystemOverview,
  planOptimization,
  rollbackOptimization,
  scanCleanup,
  scanOptimization,
  setPowerPlan,
  undoCleanup,
} from '@/services/optimizationCommands'

describe('optimization commands (B10 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'optimization.scan':
          return {
            ok: true,
            data: {
              scan: {
                hardware: { ramTotalMb: 16384, cpuThreads: 12 },
                memory: {},
                profile: 'performance',
                warnings: [],
                bottlenecks: [],
                instance: null,
                currentProfile: null,
                profiles: [{ id: 'balanced', labelKey: 'balanced', descKey: 'balancedDesc' }],
              },
            },
            warnings: [],
          }
        case 'optimization.plan':
          return {
            ok: true,
            data: {
              plan: {
                instanceId: 'i1',
                profileId: 'performance',
                jvm: [{ field: 'memory', before: { maxMb: 2048 }, after: { maxMb: 4096 } }],
                minecraft: [{ field: 'renderDistance', before: '12', after: '8' }],
                hasChanges: true,
              },
            },
            warnings: [],
          }
        case 'optimization.apply':
          return {
            ok: true,
            data: {
              applied: {
                snapshot: { file: 'opt-1.json', appliedAt: 1 },
                plan: { instanceId: 'i1', profileId: 'performance', jvm: [], minecraft: [], hasChanges: false },
                profile: 'performance',
              },
            },
            warnings: [],
          }
        case 'optimization.rollback':
          return { ok: true, data: { restored: true }, warnings: [] }
        case 'optimization.snapshot_info':
          return {
            ok: true,
            data: { snapshot: { file: 'opt-1.json', appliedAt: 1, profile: 'performance', instanceFields: ['memory'] } },
            warnings: [],
          }
        case 'system.overview':
          return {
            ok: true,
            data: {
              overview: {
                hardware: { ramTotalMb: 16384, cpuThreads: 12, ramPercent: 57.4 },
                power: { plan: 'balanced', plans: [{ id: 'balanced' }] },
                cleanupPreview: { items: [{ path: 'logs', bytes: 1024 }], totalBytes: 1024 },
              },
            },
            warnings: [],
          }
        case 'system.cleanup.scan':
          return { ok: true, data: { preview: { items: [], totalBytes: 0 } }, warnings: [] }
        case 'system.cleanup.clean':
          return { ok: true, data: { clean: { bytes: 2048, cleanId: 'c-1' } }, warnings: [] }
        case 'system.cleanup.undo':
          return { ok: true, data: { undo: { restored: 3 } }, warnings: [] }
        case 'system.cleanup.empty_trash':
          return { ok: true, data: { removed: 5 }, warnings: [] }
        case 'system.power.status':
          return { ok: true, data: { power: { plan: 'balanced' } }, warnings: [] }
        case 'system.power.set_plan':
          return { ok: true, data: { set: true }, warnings: [] }
        default:
          return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method }, warnings: [] }
      }
    })
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('scanOptimization trả hardware + profiles', async () => {
    const scan = await scanOptimization('i1')
    expect(scan.hardware.ramTotalMb).toBe(16384)
    expect(scan.profiles[0]?.id).toBe('balanced')
  })

  it('planOptimization trả diff plan', async () => {
    const plan = await planOptimization('i1', 'performance')
    expect(plan.hasChanges).toBe(true)
    expect(plan.jvm[0]?.field).toBe('memory')
    expect(plan.minecraft[0]?.before).toBe('12')
  })

  it('applyOptimization trả snapshot + plan', async () => {
    const result = await applyOptimization('i1', 'performance')
    expect(result.snapshot.file).toBe('opt-1.json')
    expect(result.profile).toBe('performance')
  })

  it('rollbackOptimization ok', async () => {
    await expect(rollbackOptimization('i1')).resolves.toBeUndefined()
  })

  it('getOptimizationSnapshot trả snapshot', async () => {
    const snapshot = await getOptimizationSnapshot('i1')
    expect(snapshot?.profile).toBe('performance')
  })

  it('getSystemOverview trả hardware + power + cleanup', async () => {
    const overview = await getSystemOverview()
    expect(overview.hardware.cpuThreads).toBe(12)
    expect(overview.cleanupPreview.items[0]?.path).toBe('logs')
  })

  it('scanCleanup + cleanPaths + undoCleanup', async () => {
    const preview = await scanCleanup()
    expect(preview.totalBytes).toBe(0)
    const clean = await cleanPaths(['logs'])
    expect(clean.cleanId).toBe('c-1')
    const undo = await undoCleanup('c-1')
    expect((undo as Record<string, unknown>).restored).toBe(3)
  })

  it('emptyTrash trả số item', async () => {
    const removed = await emptyTrash()
    expect(removed).toBe(5)
  })

  it('getPowerStatus + setPowerPlan', async () => {
    const power = await getPowerStatus()
    expect(power.plan).toBe('balanced')
    await expect(setPowerPlan('high_performance')).resolves.toBeUndefined()
  })
})
