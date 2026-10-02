/**
 * B9 — Visual Studio (§129–§135).
 * Mirror services/visuals/service.py + crosshair/totem/hud/fx payloads.
 */

/** §134 — crosshair spec (mirror crosshair.DEFAULT_SPEC). */
export interface CrosshairSpec {
  shape: 'cross' | 'circle' | 'dot' | string
  size: number
  thickness: number
  gap: number
  color: string
  outline: boolean
  outlineColor: string
  dot: boolean
  opacity: number
}

/** §9 — totem spec (mirror totem.DEFAULT_SPEC). */
export interface TotemSpec {
  base: string
  accent: string
  eye: string
  glow: boolean
  wing: boolean
}

export interface VisualPreset {
  id: string
  spec: Record<string, unknown>
}

export interface VisualPresets {
  presets: VisualPreset[]
  default: Record<string, unknown>
}

/** §129 — voxel model 3D draft (model3d.validate findings kèm theo). */
export interface TotemModelState {
  spec: Record<string, unknown> | null
  corrupt?: boolean
  findings?: Array<{ code: string; severity: string; detail?: string }>
}

/** §135 — HUD widget layout entry. */
export interface HudWidgetLayout {
  id: string
  x: number
  y: number
  scale: number
  visible: boolean
}

export interface HudWidgets {
  widgets: string[]
  defaultLayout: HudWidgetLayout[]
}

/** §52 — FX defaults (hit + particle). */
export interface FxDefaults {
  hit: { default: Record<string, unknown>; kinds: string[] }
  particle: { default: Record<string, unknown>; shapes: string[]; maxFrames: number }
}

/** Kết quả export pack: projectId + build manifest + (tuỳ chọn) install. */
export interface VisualExportResult {
  projectId: string
  build: {
    file: string
    sha256: string
    bytes: number
    files: number
    packFormat?: number
    [key: string]: unknown
  }
  spec?: Record<string, unknown>
  visuals?: Record<string, unknown>
  install?: {
    file: string
    bytes: number
    sha256: string
    backup?: string | null
    [key: string]: unknown
  }
}
