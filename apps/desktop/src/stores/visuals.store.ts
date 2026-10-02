import { defineStore } from 'pinia'

import {
  exportCrosshairPack,
  exportFxPack,
  exportTotemPack,
  getCrosshairPresets,
  getFxDefaults,
  getHudWidgets,
  getTotemModel,
  getTotemPresets,
  renderCrosshairPreview,
  renderHitPreview,
  renderParticlePreview,
  renderTotemPreview,
  saveHudLayout,
  saveTotemModel,
} from '@/services/visualsCommands'
import { toAntaresError } from '@/services/ipc'
import type {
  FxDefaults,
  HudWidgets,
  HudWidgetLayout,
  TotemModelState,
  VisualPresets,
} from '@/types/visuals'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'
export type VisualTab = 'crosshair' | 'totem' | 'totem3d' | 'hud' | 'fx'

function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

interface VisualsState {
  phase: LoadPhase
  tab: VisualTab
  crosshairPresets: VisualPresets | null
  totemPresets: VisualPresets | null
  hud: HudWidgets | null
  fx: FxDefaults | null
  /** Spec làm việc hiện tại của từng editor (bắt đầu từ default). */
  crosshairSpec: Record<string, unknown> | null
  totemSpec: Record<string, unknown> | null
  hitSpec: Record<string, unknown> | null
  particleSpec: Record<string, unknown> | null
  layout: HudWidgetLayout[] | null
  /** Preview data URI theo tab — RAF-throttled ở UI (§132). */
  preview: Record<VisualTab, string | null>
  previewPending: VisualTab | null
  hudProjectId: string
  exporting: boolean
  lastExportProjectId: string | null
  /** §129 — totem 3D draft model (VoxelSpec từ bridge). */
  totemModel: TotemModelState | null
  totemModelLoading: boolean
  totemModelSaving: boolean
  errorMessage: string | null
  errorCode: string | null
}

/**
 * B9 — Visual Studio store: presets + preview render (data URI) + export
 * pack qua pipeline Resource Studio (§133). Preview throttle ở UI (§132).
 */
export const useVisualsStore = defineStore('visuals', {
  state: (): VisualsState => ({
    phase: 'idle',
    tab: 'crosshair',
    crosshairPresets: null,
    totemPresets: null,
    hud: null,
    fx: null,
    crosshairSpec: null,
    totemSpec: null,
    hitSpec: null,
    particleSpec: null,
    layout: null,
    preview: { crosshair: null, totem: null, totem3d: null, hud: null, fx: null },
    previewPending: null,
    hudProjectId: '',
    exporting: false,
    lastExportProjectId: null,
    totemModel: null,
    totemModelLoading: false,
    totemModelSaving: false,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    currentPreview(state): string | null {
      return state.preview[state.tab]
    },
  },
  actions: {
    setTab(tab: VisualTab): void {
      this.tab = tab
    },
    async load(): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        const [crosshairPresets, totemPresets, hud, fx] = await Promise.all([
          getCrosshairPresets(),
          getTotemPresets(),
          getHudWidgets(),
          getFxDefaults(),
        ])
        this.crosshairPresets = crosshairPresets
        this.totemPresets = totemPresets
        this.hud = hud
        this.fx = fx
        this.crosshairSpec = crosshairPresets.default
        this.totemSpec = totemPresets.default
        this.hitSpec = fx.hit.default
        this.particleSpec = fx.particle.default
        this.layout = hud.defaultLayout
        this.phase = 'ready'
        void this.refreshPreview('crosshair')
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    selectPreset(presetId: string): void {
      const preset = this.crosshairPresets?.presets.find((p) => p.id === presetId)
        ?? this.totemPresets?.presets.find((p) => p.id === presetId)
      if (!preset) return
      if (this.tab === 'crosshair') {
        this.crosshairSpec = { ...preset.spec }
        void this.refreshPreview('crosshair')
      } else if (this.tab === 'totem') {
        this.totemSpec = { ...preset.spec }
        void this.refreshPreview('totem')
      }
    },
    patchSpec(patch: Record<string, unknown>): void {
      if (this.tab === 'crosshair' && this.crosshairSpec) {
        this.crosshairSpec = { ...this.crosshairSpec, ...patch }
        void this.refreshPreview('crosshair')
      } else if (this.tab === 'totem' && this.totemSpec) {
        this.totemSpec = { ...this.totemSpec, ...patch }
        void this.refreshPreview('totem')
      } else if (this.tab === 'fx') {
        if (this.hitSpec) {
          this.hitSpec = { ...this.hitSpec, ...patch }
          void this.refreshPreview('fx')
        }
      }
    },
    /** §132 — preview render theo state; UI gọi qua RAF debounce. */
    async refreshPreview(which?: VisualTab): Promise<void> {
      const target = which ?? this.tab
      if (this.previewPending === target) return
      this.previewPending = target
      try {
        if (target === 'crosshair' && this.crosshairSpec) {
          this.preview.crosshair = await renderCrosshairPreview(this.crosshairSpec)
        } else if (target === 'totem' && this.totemSpec) {
          this.preview.totem = await renderTotemPreview(this.totemSpec)
        } else if (target === 'fx' && this.hitSpec) {
          this.preview.fx = await renderHitPreview(this.hitSpec)
        }
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.previewPending = null
      }
    },
    async refreshParticlePreview(): Promise<void> {
      if (!this.particleSpec) return
      try {
        this.preview.fx = await renderParticlePreview(this.particleSpec)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    /** §129 — draft voxel model 3D (backend-backed, mục 62). */
    async loadTotemModel(): Promise<void> {
      this.totemModelLoading = true
      try {
        this.totemModel = await getTotemModel()
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.totemModelLoading = false
      }
    },
    async persistTotemModel(spec: Record<string, unknown>): Promise<boolean> {
      this.totemModelSaving = true
      try {
        const result = await saveTotemModel(spec)
        const next: TotemModelState = { spec }
        if (result.findings !== undefined) next.findings = result.findings
        this.totemModel = next
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.totemModelSaving = false
      }
    },
    async persistHudLayout(projectId: string): Promise<boolean> {
      if (!this.layout) return false
      try {
        const result = await saveHudLayout(projectId, this.layout)
        this.layout = result.layout
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async export(tab: VisualTab, params: {
      name: string
      mcVersion: string
      installInstanceId?: string
      overwrite?: boolean
    }): Promise<boolean> {
      this.exporting = true
      try {
        let result
        if (tab === 'crosshair') {
          if (!this.crosshairSpec) return false
          result = await exportCrosshairPack({ ...params, spec: this.crosshairSpec })
        } else if (tab === 'totem') {
          if (!this.totemSpec) return false
          result = await exportTotemPack({ ...params, spec: this.totemSpec })
        } else if (tab === 'fx') {
          if (!this.hitSpec && !this.particleSpec) return false
          const { name, mcVersion, installInstanceId, overwrite } = params
          const fxParams: {
            name: string
            mcVersion: string
            hitSpec?: Record<string, unknown>
            particleSpec?: Record<string, unknown>
            installInstanceId?: string
            overwrite?: boolean
          } = { name, mcVersion }
          if (installInstanceId !== undefined) fxParams.installInstanceId = installInstanceId
          if (overwrite !== undefined) fxParams.overwrite = overwrite
          if (this.hitSpec) fxParams.hitSpec = this.hitSpec
          if (this.particleSpec) fxParams.particleSpec = this.particleSpec
          result = await exportFxPack(fxParams)
        } else {
          return false
        }
        this.lastExportProjectId = result.projectId
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.exporting = false
      }
    },
  },
})
