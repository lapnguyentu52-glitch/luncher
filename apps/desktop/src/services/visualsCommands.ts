import { callLegacy } from './legacyCommands'
import type {
  FxDefaults,
  HudWidgets,
  HudWidgetLayout,
  TotemModelState,
  VisualExportResult,
  VisualPresets,
} from '@/types/visuals'

/**
 * B9 — Visual Studio qua legacy bridge (§129–§135).
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

// ---- Crosshair (§134) ----

export async function getCrosshairPresets(): Promise<VisualPresets> {
  return callLegacy<VisualPresets>('visual.presets')
}

/** Preview PNG 16×16 (data URI) — UI throttle theo RAF (§132). */
export async function renderCrosshairPreview(spec: Record<string, unknown>): Promise<string> {
  const data = await callLegacy<{ preview: string }>('visual.render_preview', { spec })
  return data.preview
}

export async function exportCrosshairPack(params: {
  name: string
  mcVersion: string
  spec: Record<string, unknown>
  installInstanceId?: string
  overwrite?: boolean
}): Promise<VisualExportResult> {
  const data = await callLegacy<{ export: VisualExportResult }>('visual.export_pack', params)
  return data.export
}

// ---- Totem (§129/§130) ----

export async function getTotemPresets(): Promise<VisualPresets> {
  return callLegacy<VisualPresets>('visual.totem_presets')
}

export async function renderTotemPreview(spec: Record<string, unknown>): Promise<string> {
  const data = await callLegacy<{ preview: string }>('visual.render_totem', { spec })
  return data.preview
}

export async function getTotemModel(): Promise<TotemModelState> {
  return callLegacy<TotemModelState>('visual.totem_model.get')
}

export async function saveTotemModel(spec: Record<string, unknown>): Promise<{ saved: boolean; findings: TotemModelState['findings'] }> {
  return callLegacy<{ saved: boolean; findings: TotemModelState['findings'] }>(
    'visual.totem_model.save',
    { spec },
  )
}

/** Static render 3D (§130) — fallback khi WebGL không khả dụng. */
export async function renderTotemModel(spec: Record<string, unknown>, size = 256): Promise<string> {
  const data = await callLegacy<{ preview: string }>('visual.render_totem_model', { spec, size })
  return data.preview
}

export async function exportTotemPack(params: {
  name: string
  mcVersion: string
  spec: Record<string, unknown>
  installInstanceId?: string
  overwrite?: boolean
}): Promise<VisualExportResult> {
  const data = await callLegacy<{ export: VisualExportResult }>('visual.export_totem_pack', params)
  return data.export
}

// ---- HUD (§135) ----

export async function getHudWidgets(): Promise<HudWidgets> {
  return callLegacy<HudWidgets>('visual.hud_widgets')
}

export async function saveHudLayout(projectId: string, layout: HudWidgetLayout[]): Promise<{ layout: HudWidgetLayout[] }> {
  return callLegacy<{ layout: HudWidgetLayout[] }>('visual.save_hud_layout', {
    projectId,
    layout,
  })
}

// ---- FX (§52) ----

export async function getFxDefaults(): Promise<FxDefaults> {
  return callLegacy<FxDefaults>('visual.fx_defaults')
}

export async function renderHitPreview(spec: Record<string, unknown>): Promise<string> {
  const data = await callLegacy<{ preview: string }>('visual.render_hit', { spec })
  return data.preview
}

export async function renderParticlePreview(spec: Record<string, unknown>): Promise<string> {
  const data = await callLegacy<{ preview: string }>('visual.render_particle', { spec })
  return data.preview
}

export async function exportFxPack(params: {
  name: string
  mcVersion: string
  hitSpec?: Record<string, unknown>
  particleSpec?: Record<string, unknown>
  installInstanceId?: string
  overwrite?: boolean
}): Promise<VisualExportResult> {
  const data = await callLegacy<{ export: VisualExportResult }>('visual.export_fx_pack', params)
  return data.export
}
