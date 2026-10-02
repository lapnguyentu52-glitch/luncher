import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  buildProject,
  createResourceProject,
  deleteResourceProject,
  generateProject,
  getLayerState,
  getWizardInfo,
  installPack,
  listBuilds,
  listInstalledPacks,
  moveLayerPack,
  setLayerOrder,
  uninstallPack,
  updateResourceProject,
  validateProject,
} from '@/services/resourcesCommands'
import type { BuildManifest } from '@/types/resources'

const demoManifest: BuildManifest = {
  project: 'p1',
  name: 'My Pack',
  mcVersion: '1.21.4',
  packFormat: 46,
  packMetaMode: 'pack_format',
  file: 'my-pack-1760000.zip',
  sha256: 'ab'.repeat(32),
  bytes: 1024,
  files: 7,
  warnings: [],
  builtAt: 1760000,
}

describe('resources commands (B8b smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'resource.wizard_info':
          return {
            ok: true,
            data: {
              templates: [{ id: 'minimal', labelKey: 'minimal', descKey: 'minimalDesc', modules: { crosshair: true } }],
              versions: ['1.21.11', '1.21.4'],
              defaultVersion: '1.21.11',
            },
            warnings: [],
          }
        case 'resource.create':
          return {
            ok: true,
            data: { project: { id: 'p1', name: String(a.params?.name), minecraft: { version: '1.21.4' }, createdAt: 1, updatedAt: 1 } },
            warnings: [],
          }
        case 'resource.update':
          return {
            ok: true,
            data: { project: { id: 'p1', name: 'Renamed', minecraft: { version: '1.21.4' }, createdAt: 1, updatedAt: 2 } },
            warnings: [],
          }
        case 'resource.delete':
          return { ok: true, data: { deleted: true }, warnings: [] }
        case 'resource.generate':
          return { ok: true, data: { generated: { files: 5, mcVersion: '1.21.4', packFormat: 46 } }, warnings: [] }
        case 'resource.validate':
          return {
            ok: true,
            data: {
              ok: true,
              findings: [{ code: 'duplicate_file', severity: 'WARNING', path: 'pack.png', detail: 'dup' }],
              packFormat: 46,
            },
            warnings: [],
          }
        case 'resource.build':
          return { ok: true, data: { manifest: demoManifest }, warnings: [] }
        case 'resource.builds':
          return { ok: true, data: { builds: [demoManifest] }, warnings: [] }
        case 'resource.install':
          return {
            ok: true,
            data: { installed: { file: 'my-pack.zip', bytes: 1024, sha256: 'ab', backup: null, installedAt: 1 } },
            warnings: [],
          }
        case 'resource.installed':
          return { ok: true, data: { packs: [{ file: 'my-pack.zip', bytes: 1024 }] }, warnings: [] }
        case 'resource.uninstall':
          return { ok: true, data: { removed: true }, warnings: [] }
        case 'resource.layer.get':
          return {
            ok: true,
            data: {
              order: ['a.zip', 'b.zip'],
              preview: { order: ['a.zip', 'b.zip'], assets: {}, conflicts: [] },
            },
            warnings: [],
          }
        case 'resource.layer.set':
          return { ok: true, data: { order: ['b.zip', 'a.zip'] }, warnings: [] }
        case 'resource.layer.move':
          return { ok: true, data: { order: ['b.zip', 'a.zip'] }, warnings: [] }
        default:
          return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method }, warnings: [] }
      }
    })
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('getWizardInfo trả templates + versions', async () => {
    const info = await getWizardInfo()
    expect(info.defaultVersion).toBe('1.21.11')
    expect(info.templates[0]?.modules.crosshair).toBe(true)
  })

  it('createResourceProject trả project', async () => {
    const project = await createResourceProject({ name: 'My Pack', mcVersion: '1.21.4' })
    expect(project.minecraft.version).toBe('1.21.4')
  })

  it('updateResourceProject trả project mới', async () => {
    const project = await updateResourceProject('p1', { name: 'Renamed' })
    expect(project.name).toBe('Renamed')
  })

  it('deleteResourceProject gửi confirm:true', async () => {
    await expect(deleteResourceProject('p1')).resolves.toBeUndefined()
  })

  it('generateProject trả files + packFormat', async () => {
    const result = await generateProject('p1')
    expect(result.files).toBe(5)
    expect(result.packFormat).toBe(46)
  })

  it('validateProject trả findings + packFormat', async () => {
    const result = await validateProject('p1')
    expect(result.ok).toBe(true)
    expect(result.findings[0]?.severity).toBe('WARNING')
    expect(result.packFormat).toBe(46)
  })

  it('buildProject trả manifest', async () => {
    const manifest = await buildProject('p1')
    expect(manifest.file.endsWith('.zip')).toBe(true)
    expect(manifest.sha256).toHaveLength(64)
  })

  it('listBuilds trả builds', async () => {
    const builds = await listBuilds('p1')
    expect(builds[0]?.file).toBe(demoManifest.file)
  })

  it('installPack trả install result', async () => {
    const result = await installPack({ projectId: 'p1', instanceId: 'i1', overwrite: true })
    expect(result.file).toBe('my-pack.zip')
  })

  it('listInstalledPacks trả packs', async () => {
    const packs = await listInstalledPacks('i1')
    expect(packs[0]?.file).toBe('my-pack.zip')
  })

  it('uninstallPack ok', async () => {
    await expect(uninstallPack('i1', 'my-pack.zip')).resolves.toBeUndefined()
  })

  it('getLayerState trả order + preview', async () => {
    const layer = await getLayerState('i1')
    expect(layer.order).toEqual(['a.zip', 'b.zip'])
    expect(layer.preview.conflicts).toEqual([])
  })

  it('setLayerOrder + moveLayerPack trả order mới', async () => {
    const afterSet = await setLayerOrder('i1', ['b.zip', 'a.zip'])
    expect(afterSet).toEqual(['b.zip', 'a.zip'])
    const afterMove = await moveLayerPack('i1', 'b.zip', -1)
    expect(afterMove).toEqual(['b.zip', 'a.zip'])
  })
})
