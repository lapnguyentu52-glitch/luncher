/**
 * B8a — Asset Library + Resource Studio projects (§12/§123–§125).
 * Mirror services/resources/assets.py + services/resources/project.py payloads.
 */

export interface AssetEntry {
  id: string
  name: string
  sha256: string
  mime: string
  width: number
  height: number
  bytes: number
  category: string
  tags: string[]
  source: string
  createdAt: number
  updatedAt: number
}

export interface AssetWithPreview {
  asset: AssetEntry
  /** data:image/png;base64,… — dùng trực tiếp trong <img :src>. */
  preview: string
}

/** §123 — Resource project (mirror project.json). */
export interface ResourceProject {
  id: string
  name: string
  description?: string
  minecraft: { version: string }
  template?: string
  buildCount?: number
  assets?: Array<{ assetId: string; sha256: string; path: string; assignedAt: number }>
  createdAt: number
  updatedAt: number
}

/** Asset categories phía service (§12). */
export const ASSET_CATEGORIES = [
  'item',
  'block',
  'entity',
  'gui',
  'environment',
  'misc',
] as const

export type AssetCategory = (typeof ASSET_CATEGORIES)[number]
