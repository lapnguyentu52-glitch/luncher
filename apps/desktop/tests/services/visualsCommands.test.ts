import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  exportCrosshairPack,
  exportFxPack,
  exportTotemPack,
  getCrosshairPresets,
  getFxDefaults,
  getHudWidgets,
  getTotemPresets,
  renderCrosshairPreview,
  renderHitPreview,
  renderParticlePreview,
  renderTotemModel,
  renderTotemPreview,
  saveHudLayout,
} from '@/services/visualsCommands'

const DATA_URI = 'data:image/png;base64,AAAA'

describe('visuals commands (B9 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'visual.presets':
          return {
            ok: true,
            data: {
              presets: [{ id: 'minimal', spec: { shape: 'cross' } }],
              default: { shape: 'cross', size: 16 },
            },
            warnings: [],
          }
        case 'visual.render_preview':
          return { ok: true, data: { preview: DATA_URI }, warnings: [] }
        case 'visual.export_pack':
          return {
            ok: true,
            data: { export: { projectId: 'p1', build: { file: 'x.zip', sha256: 'ab', bytes: 1, files: 1 } } },
            warnings: [],
          }
        case 'visual.totem_presets':
          return {
            ok: true,
            data: {
              presets: [{ id: 'classic', spec: { base: '#e8b23a' } }],
              default: { base: '#e8b23a' },
            },
            warnings: [],
          }
        case 'visual.render_totem':
          return { ok: true, data: { preview: DATA_URI }, warnings: [] }
        case 'visual.render_totem_model':
          return { ok: true, data: { preview: DATA_URI }, warnings: [] }
        case 'visual.export_totem_pack':
          return {
            ok: true,
            data: { export: { projectId: 'p2', build: { file: 't.zip', sha256: 'ab', bytes: 1, files: 1 } } },
            warnings: [],
          }
        case 'visual.hud_widgets':
          return {
            ok: true,
            data: {
              widgets: ['fps', 'cps'],
              defaultLayout: [{ id: 'fps', x: 4, y: 4, scale: 1, visible: true }],
            },
            warnings: [],
          }
        case 'visual.save_hud_layout':
          return {
            ok: true,
            data: { layout: [{ id: 'fps', x: 4, y: 4, scale: 1, visible: true }] },
            warnings: [],
          }
        case 'visual.fx_defaults':
          return {
            ok: true,
            data: {
              hit: { default: { kind: 'flash' }, kinds: ['none', 'flash'] },
              particle: { default: { shape: 'orb' }, shapes: ['orb'], maxFrames: 8 },
            },
            warnings: [],
          }
        case 'visual.render_hit':
        case 'visual.render_particle':
          return { ok: true, data: { preview: DATA_URI }, warnings: [] }
        case 'visual.export_fx_pack':
          return {
            ok: true,
            data: { export: { projectId: 'p3', build: { file: 'f.zip', sha256: 'ab', bytes: 1, files: 1 } } },
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

  it('getCrosshairPresets trả presets + default', async () => {
    const presets = await getCrosshairPresets()
    expect(presets.presets[0]?.id).toBe('minimal')
    expect(presets.default.shape).toBe('cross')
  })

  it('renderCrosshairPreview trả data URI', async () => {
    const preview = await renderCrosshairPreview({ shape: 'cross' })
    expect(preview.startsWith('data:image/png')).toBe(true)
  })

  it('exportCrosshairPack trả export result', async () => {
    const result = await exportCrosshairPack({
      name: 'My Cross',
      mcVersion: '1.21.4',
      spec: { shape: 'cross' },
    })
    expect(result.projectId).toBe('p1')
    expect(result.build.file.endsWith('.zip')).toBe(true)
  })

  it('getTotemPresets + renderTotemPreview', async () => {
    const presets = await getTotemPresets()
    expect(presets.presets[0]?.id).toBe('classic')
    const preview = await renderTotemPreview({ base: '#e8b23a' })
    expect(preview.startsWith('data:image/png')).toBe(true)
  })

  it('renderTotemModel trả data URI', async () => {
    const preview = await renderTotemModel({ version: 1 }, 256)
    expect(preview.startsWith('data:image/png')).toBe(true)
  })

  it('exportTotemPack trả export result', async () => {
    const result = await exportTotemPack({
      name: 'My Totem',
      mcVersion: '1.21.4',
      spec: { base: '#e8b23a' },
    })
    expect(result.projectId).toBe('p2')
  })

  it('getHudWidgets + saveHudLayout', async () => {
    const widgets = await getHudWidgets()
    expect(widgets.widgets).toContain('fps')
    const saved = await saveHudLayout('p1', widgets.defaultLayout)
    expect(saved.layout[0]?.id).toBe('fps')
  })

  it('getFxDefaults trả hit + particle', async () => {
    const fx = await getFxDefaults()
    expect(fx.hit.kinds).toContain('flash')
    expect(fx.particle.maxFrames).toBe(8)
  })

  it('renderHitPreview + renderParticlePreview', async () => {
    const hit = await renderHitPreview({ kind: 'flash' })
    const particle = await renderParticlePreview({ shape: 'orb' })
    expect(hit.startsWith('data:image/png')).toBe(true)
    expect(particle.startsWith('data:image/png')).toBe(true)
  })

  it('exportFxPack trả export result', async () => {
    const result = await exportFxPack({
      name: 'My FX',
      mcVersion: '1.21.4',
      hitSpec: { kind: 'flash' },
    })
    expect(result.projectId).toBe('p3')
  })
})
