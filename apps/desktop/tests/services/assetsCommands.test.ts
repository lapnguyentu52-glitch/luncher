import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  assignAsset,
  deleteAsset,
  getAsset,
  importAsset,
  listAssets,
  listResourceProjects,
} from '@/services/assetsCommands'
import type { AssetEntry, ResourceProject } from '@/types/assets'

const demoAsset: AssetEntry = {
  id: 'ab12cd34ef56',
  name: 'sword.png',
  sha256: 'ab'.repeat(32),
  mime: 'image/png',
  width: 16,
  height: 16,
  bytes: 428,
  category: 'item',
  tags: ['weapon'],
  source: 'imported',
  createdAt: 1759000000,
  updatedAt: 1759000000,
}

const demoProject: ResourceProject = {
  id: 'p1',
  name: 'My Pack',
  minecraft: { version: '1.21.4' },
  createdAt: 1759000000,
  updatedAt: 1759000000,
}

describe('assets commands (B8a smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'asset.list':
          return { ok: true, data: { assets: [demoAsset] }, warnings: [] }
        case 'asset.import':
          return { ok: true, data: { asset: { ...demoAsset, name: String(a.params?.name) } }, warnings: [] }
        case 'asset.get':
          return {
            ok: true,
            data: { asset: demoAsset, preview: 'data:image/png;base64,AAAA' },
            warnings: [],
          }
        case 'asset.delete':
          return { ok: true, data: { deleted: true }, warnings: [] }
        case 'asset.assign':
          return {
            ok: true,
            data: { assigned: { path: String(a.params?.targetRel), asset: 'sword.png', sha256: demoAsset.sha256 } },
            warnings: [],
          }
        case 'resource.list':
          return { ok: true, data: { projects: [demoProject] }, warnings: [] }
        default:
          return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method }, warnings: [] }
      }
    })
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('listAssets trả catalog', async () => {
    const assets = await listAssets({ query: 'sw', sort: 'name' })
    expect(assets[0]?.name).toBe('sword.png')
  })

  it('importAsset trả entry', async () => {
    const asset = await importAsset({ dataB64: 'AAAA', name: 'sword.png' })
    expect(asset.width).toBe(16)
  })

  it('getAsset trả asset + preview data URI', async () => {
    const result = await getAsset('ab12cd34ef56')
    expect(result.preview.startsWith('data:image/png')).toBe(true)
  })

  it('deleteAsset ok', async () => {
    await expect(deleteAsset('ab12cd34ef56')).resolves.toBeUndefined()
  })

  it('assignAsset trả path/asset/sha256', async () => {
    const assigned = await assignAsset({
      projectId: 'p1',
      assetId: 'ab12cd34ef56',
      targetRel: 'assets/minecraft/textures/item/sword.png',
    })
    expect(assigned.path).toBe('assets/minecraft/textures/item/sword.png')
  })

  it('listResourceProjects trả projects', async () => {
    const projects = await listResourceProjects()
    expect(projects[0]?.minecraft.version).toBe('1.21.4')
  })
})
