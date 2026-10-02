import { defineStore } from 'pinia'

export type AccentPreset = 'red' | 'blue' | 'green' | 'violet' | 'amber' | 'custom'

export interface AppearanceSettings {
  accent: AccentPreset
  /** Hex khi accent === 'custom' (từ color picker) */
  customAccent: string
  /** Preset nền: aurora mặc định / đơn sắc / tối hơn / custom / ảnh */
  background: 'aurora' | 'plain' | 'deep' | 'custom' | 'image'
  /** Hex glow màu chính của nền custom */
  customBgColor: string
  /** data URL ảnh nền (PNG/JPG base64) — dùng khi background === 'image' */
  bgImageData: string | null
  /** 0..0.85 — overlay tối trên ảnh nền để chữ đọc được */
  bgDim: number
  /** 0..24px — blur backdrop-filter của khung kính */
  glassBlur: number
  /** 0.4..0.95 — alpha nền kính (càng thấp càng trong suốt) */
  glassOpacity: number
  /** 200..320 — độ rộng sidebar khi mở */
  sidebarWidth: number
  density: 'comfortable' | 'compact'
  reducedMotion: boolean
  reducedTransparency: boolean
}

const STORAGE_KEY = 'antares.settings.v2'

/** Accent hex theo preset — dùng bởi applyTheme() ghi CSS vars. */
export const ACCENT_PRESETS: Record<Exclude<AccentPreset, 'custom'>, string> = {
  red: '#ff5c47',
  blue: '#5b9dff',
  green: '#50d890',
  violet: '#9e7aff',
  amber: '#ffc857',
}

export const BG_PRESETS: Record<
  Exclude<AppearanceSettings['background'], 'custom' | 'image'>,
  { accent: string; info: string; violet: string }
> = {
  aurora: {
    accent: 'rgba(255, 92, 71, 0.13)',
    info: 'rgba(98, 168, 255, 0.10)',
    violet: 'rgba(158, 122, 255, 0.08)',
  },
  plain: {
    accent: 'rgba(128, 128, 128, 0.05)',
    info: 'rgba(128, 128, 128, 0.05)',
    violet: 'rgba(128, 128, 128, 0.04)',
  },
  deep: {
    accent: 'rgba(255, 92, 71, 0.06)',
    info: 'rgba(98, 168, 255, 0.05)',
    violet: 'rgba(158, 122, 255, 0.04)',
  },
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function loadPersisted(): Partial<AppearanceSettings> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw === null) return {}
    const parsed: unknown = JSON.parse(raw)
    if (!isRecord(parsed)) return {}
    return parsed as Partial<AppearanceSettings>
  } catch {
    return {}
  }
}

function safeHex(v: unknown, fallback: string): string {
  return typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v) ? v : fallback
}

function clamp(v: unknown, lo: number, hi: number, fallback: number): number {
  const n = typeof v === 'number' && Number.isFinite(v) ? v : fallback
  return Math.min(hi, Math.max(lo, n))
}

function hexToRgba(hex: string, alpha: number): string {
  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

/** Ghi CSS variables từ settings — gọi khi settings đổi (App.vue watch). */
export function applyTheme(s: AppearanceSettings): void {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  const accent = s.accent === 'custom' ? s.customAccent : ACCENT_PRESETS[s.accent]
  root.style.setProperty('--accent', accent)
  root.style.setProperty('--accent-soft', hexToRgba(accent, 0.14))
  root.style.setProperty('--accent-glow', hexToRgba(accent, 0.22))

  const bg =
    s.background === 'custom'
      ? {
          accent: hexToRgba(s.customBgColor, 0.13),
          info: hexToRgba(s.customBgColor, 0.1),
          violet: hexToRgba(s.customBgColor, 0.08),
        }
      : BG_PRESETS[s.background === 'image' ? 'aurora' : s.background]
  root.style.setProperty('--aurora-accent', bg.accent)
  root.style.setProperty('--aurora-info', bg.info)
  root.style.setProperty('--aurora-violet', bg.violet)

  // Kính: blur + alpha từ settings (giảm transparency → đặc hơn, bỏ blur)
  const alpha = s.reducedTransparency ? Math.max(s.glassOpacity, 0.9) : s.glassOpacity
  const base = Math.round(11 + (1 - alpha) * 6)
  root.style.setProperty('--glass-bg', `rgba(${base}, ${base + 4}, ${base + 12}, ${alpha})`)
  root.style.setProperty('--glass-bg-strong', `rgba(${base - 2}, ${base + 2}, ${base + 10}, ${Math.min(0.96, alpha + 0.14)})`)
  // Frosted solid cho nội dung cuộn — luôn đặc (≥0.9), không backdrop-filter
  root.style.setProperty('--glass-bg-solid', `rgba(${base - 2}, ${base + 2}, ${base + 10}, ${Math.max(0.9, alpha)})`)
  root.style.setProperty('--glass-blur', `${s.reducedTransparency ? 0 : s.glassBlur}px`)

  // Sidebar width
  root.style.setProperty('--sidebar-w', `${s.sidebarWidth}px`)

  // Ảnh nền: data URL + dim overlay
  if (s.background === 'image' && s.bgImageData) {
    root.style.setProperty('--bg-image', `url("${s.bgImageData}")`)
    root.style.setProperty('--bg-dim', String(s.bgDim))
  } else {
    root.style.setProperty('--bg-image', 'none')
    root.style.setProperty('--bg-dim', '0')
  }
}

export const useSettingsStore = defineStore('settings', {
  state: () => ({
    accent: 'red' as AccentPreset,
    customAccent: '#ff8a5c',
    background: 'aurora' as AppearanceSettings['background'],
    customBgColor: '#5b9dff',
    bgImageData: null as string | null,
    bgDim: 0.45,
    glassBlur: 16,
    glassOpacity: 0.58,
    sidebarWidth: 232,
    density: 'comfortable' as AppearanceSettings['density'],
    // Default tôn trọng OS setting
    reducedMotion:
      typeof window !== 'undefined' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches,
    reducedTransparency: false,
  }),
  getters: {
    accentHex(state): string {
      return state.accent === 'custom' ? state.customAccent : ACCENT_PRESETS[state.accent]
    },
  },
  actions: {
    hydrate(): void {
      const p = loadPersisted()
      if (p.accent !== undefined) this.accent = p.accent
      if (p.customAccent !== undefined) this.customAccent = safeHex(p.customAccent, this.customAccent)
      if (p.background !== undefined) this.background = p.background
      if (p.customBgColor !== undefined) this.customBgColor = safeHex(p.customBgColor, this.customBgColor)
      if (p.bgImageData !== undefined) this.bgImageData = typeof p.bgImageData === 'string' ? p.bgImageData : null
      if (p.bgDim !== undefined) this.bgDim = clamp(p.bgDim, 0, 0.85, 0.45)
      if (p.glassBlur !== undefined) this.glassBlur = clamp(p.glassBlur, 0, 24, 16)
      if (p.glassOpacity !== undefined) this.glassOpacity = clamp(p.glassOpacity, 0.4, 0.95, 0.58)
      if (p.sidebarWidth !== undefined) this.sidebarWidth = clamp(p.sidebarWidth, 200, 320, 232)
      if (p.density !== undefined) this.density = p.density
      if (p.reducedMotion !== undefined) this.reducedMotion = p.reducedMotion
      if (p.reducedTransparency !== undefined) this.reducedTransparency = p.reducedTransparency
    },
    persist(): void {
      const payload = {
        accent: this.accent,
        customAccent: this.customAccent,
        background: this.background,
        customBgColor: this.customBgColor,
        bgImageData: this.bgImageData,
        bgDim: this.bgDim,
        glassBlur: this.glassBlur,
        glassOpacity: this.glassOpacity,
        sidebarWidth: this.sidebarWidth,
        density: this.density,
        reducedMotion: this.reducedMotion,
        reducedTransparency: this.reducedTransparency,
      }
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(payload))
      } catch {
        // quota exceeded (ảnh lớn) — thử persist không ảnh
        try {
          const rest: Record<string, unknown> = { ...payload }
          delete rest.bgImageData
          localStorage.setItem(STORAGE_KEY, JSON.stringify(rest))
        } catch {
          /* private mode */
        }
      }
    },
    /** Đọc file ảnh → data URL base64 (giới hạn ~2MB). */
    async setBgImage(file: File): Promise<boolean> {
      if (!file.type.startsWith('image/') || file.size > 2 * 1024 * 1024) return false
      const data = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader()
        reader.onload = () => resolve(String(reader.result))
        reader.onerror = () => reject(reader.error)
        reader.readAsDataURL(file)
      })
      this.bgImageData = data
      this.background = 'image'
      return true
    },
    clearBgImage(): void {
      this.bgImageData = null
      if (this.background === 'image') this.background = 'aurora'
    },
  },
})
