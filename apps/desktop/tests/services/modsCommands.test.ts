import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  cancelModpack,
  deleteQuarantined,
  getModDetail,
  getModsHealth,
  getMrpackInfo,
  installMod,
  listInstalledMods,
  listQuarantine,
  pollModpackStatus,
  removeMod,
  scanMods,
  searchMods,
} from '@/services/modsCommands'

describe('mods commands (B7 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'mods.list':
          return {
            ok: true,
            data: { instanceId: a.params?.instanceId, mods: [{ filename: 'sodium.jar', name: 'Sodium', readable: true }] },
            warnings: [],
          }
        case 'mods.search':
          return {
            ok: true,
            data: { hits: [{ projectId: 'p1', title: 'Sodium', author: 'jellysquid', downloads: 1000 }] },
            warnings: [],
          }
        case 'mods.install':
          return { ok: true, data: { filename: 'sodium.jar' }, warnings: [] }
        case 'mods.remove':
          return { ok: true, data: { removed: true }, warnings: [] }
        case 'mods.health':
          return {
            ok: true,
            data: {
              health: {
                mods: [], outdated: [],
                issues: [{ mod: 'sodium.jar', kind: 'missing_dependency', detail: 'fabric-api', fixable: true, suggestion: 'fabric-api' }],
              },
            },
            warnings: [],
          }
        case 'mods.scan':
          return {
            ok: true,
            data: {
              scan: {
                results: [{ file: 'bad.jar', verdict: 'DANGEROUS', score: 90, findings: [], quarantined: true }],
                dangerous: 1, suspicious: 0, safe: 2,
              },
            },
            warnings: [],
          }
        case 'mods.quarantine.list':
          return {
            ok: true,
            data: { items: [{ quarantineFile: '1_bad.jar', originalName: 'bad.jar', originalPath: '/x', verdict: 'DANGEROUS', score: 90, findings: [], quarantinedAt: 1 }] },
            warnings: [],
          }
        case 'mods.quarantine.delete':
          return { ok: true, data: { deleted: true }, warnings: [] }
        case 'mod.detail':
          return {
            ok: true,
            data: {
              detail: {
                filename: 'sodium.jar', modId: 'sodium', name: 'Sodium', version: '0.6.0',
                loader: 'fabric', mcVersions: ['1.21.11'],
                depends: { 'fabric-api': '*' }, recommends: {}, breaks: { optifine: '*' },
                fabricModJson: true, forgeToml: false, readable: true,
              },
            },
            warnings: [],
          }
        case 'modpack.info':
          return {
            ok: true,
            data: {
              info: {
                name: 'Test Pack', minecraftVersion: '1.21.11', loader: 'fabric',
                loaderVersion: '0.16.9', optionalFiles: [],
              },
            },
            warnings: [],
          }
        case 'modpack.status':
          return {
            ok: true,
            data: {
              task: {
                id: a.params?.taskId, type: 'MODPACK_INSTALL', owner: 'instance:i1',
                state: 'completed', progress: 100, message: 'Done', result: { name: 'Test Pack' },
              },
            },
            warnings: [],
          }
        case 'modpack.cancel':
          return { ok: true, data: { cancelled: true }, warnings: [] }
        default:
          return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method }, warnings: [] }
      }
    })
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('listInstalledMods trả typed entries', async () => {
    const mods = await listInstalledMods('i1')
    expect(mods[0]?.filename).toBe('sodium.jar')
    expect(mods[0]?.readable).toBe(true)
  })

  it('searchMods trả hits', async () => {
    const hits = await searchMods({ query: 'sodium', loader: 'fabric', mcVersion: '1.21.11' })
    expect(hits[0]?.projectId).toBe('p1')
  })

  it('installMod trả filename', async () => {
    await expect(installMod({ projectId: 'p1', instanceId: 'i1' })).resolves.toBe('sodium.jar')
  })

  it('removeMod ok', async () => {
    await expect(removeMod('i1', 'sodium.jar')).resolves.toBeUndefined()
  })

  it('getModsHealth trả issues §162', async () => {
    const health = await getModsHealth('i1')
    expect(health.issues[0]?.kind).toBe('missing_dependency')
    expect(health.issues[0]?.fixable).toBe(true)
  })

  it('scanMods trả counts + verdicts §164', async () => {
    const scan = await scanMods('i1')
    expect(scan.dangerous).toBe(1)
    expect(scan.results[0]?.quarantined).toBe(true)
  })

  it('listQuarantine + deleteQuarantined (confirm bắt buộc phía service)', async () => {
    const items = await listQuarantine()
    expect(items[0]?.verdict).toBe('DANGEROUS')
    let captured: unknown
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      captured = (args as { params?: Record<string, unknown> }).params
      return { ok: true, data: { deleted: true }, warnings: [] }
    })
    await deleteQuarantined('1_bad.jar')
    expect((captured as Record<string, unknown>)?.confirm).toBe(true)
  })

  it('getModDetail trả depends/breaks §11', async () => {
    const detail = await getModDetail('i1', 'sodium.jar')
    expect(detail.modId).toBe('sodium')
    expect(detail.depends['fabric-api']).toBe('*')
    expect(detail.breaks.optifine).toBe('*')
  })

  it('getMrpackInfo trả preview', async () => {
    const info = await getMrpackInfo('/tmp/pack.mrpack')
    expect(info.name).toBe('Test Pack')
    expect(info.loader).toBe('fabric')
  })

  it('pollModpackStatus trả task + result', async () => {
    const task = await pollModpackStatus('task-1')
    expect(task.id).toBe('task-1')
    expect(task.state).toBe('completed')
    expect(task.result?.name).toBe('Test Pack')
  })

  it('cancelModpack ok', async () => {
    await expect(cancelModpack('task-1')).resolves.toBeUndefined()
  })
})
