import { defineStore } from 'pinia'

import {
  assignAsset,
  deleteAsset,
  getAsset,
  listAssets,
  listResourceProjects,
} from '@/services/assetsCommands'
import { toAntaresError } from '@/services/ipc'
import type { AssetEntry, AssetWithPreview, ResourceProject } from '@/types/assets'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'

function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

export type AssetSort = 'newest' | 'oldest' | 'name' | 'size'

interface AssetsState {
  phase: LoadPhase
  items: AssetEntry[]
  query: string
  category: string
  sort: AssetSort
  /** Preview cache theo assetId (data URI) — tránh re-fetch trong session. */
  previews: Record<string, string>
  previewLoadingId: string | null
  importing: boolean
  busyId: string | null
  projects: ResourceProject[]
  selectedProjectId: string | null
  errorMessage: string | null
  errorCode: string | null
}

/**
 * §124/§125 — Asset Library store: content-addressed catalog + preview cache.
 * Preview key = assetId; data không đổi vì sha256 content-addressed (§205 reuse).
 */
export const useAssetsStore = defineStore('assets', {
  state: (): AssetsState => ({
    phase: 'idle',
    items: [],
    query: '',
    category: '',
    sort: 'newest',
    previews: {},
    previewLoadingId: null,
    importing: false,
    busyId: null,
    projects: [],
    selectedProjectId: null,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    filteredCount(state): number {
      return state.items.length
    },
    selectedProject(state): ResourceProject | undefined {
      return state.projects.find((p) => p.id === state.selectedProjectId)
    },
  },
  actions: {
    async load(): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        const [assets, projects] = await Promise.all([
          listAssets({ query: this.query, category: this.category, sort: this.sort }),
          listResourceProjects(),
        ])
        this.items = assets
        this.projects = projects
        this.phase = 'ready'
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    setFilter(patch: { query?: string; category?: string; sort?: AssetSort }): void {
      if (patch.query !== undefined) this.query = patch.query
      if (patch.category !== undefined) this.category = patch.category
      if (patch.sort !== undefined) this.sort = patch.sort
      void this.load()
    },
    /** §206 — preview fetch on demand, cache trong session. */
    async loadPreview(assetId: string): Promise<string | null> {
      const cached = this.previews[assetId]
      if (cached) return cached
      this.previewLoadingId = assetId
      try {
        const result: AssetWithPreview = await getAsset(assetId)
        this.previews[assetId] = result.preview
        return result.preview
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return null
      } finally {
        this.previewLoadingId = null
      }
    },
    async importPng(file: File, category?: string, tags?: string[]): Promise<boolean> {
      this.importing = true
      try {
        const { fileToBase64, importAsset } = await import('@/services/assetsCommands')
        const dataB64 = await fileToBase64(file)
        const params: { dataB64: string; name: string; category?: string; tags?: string[] } = {
          dataB64,
          name: file.name,
        }
        const categoryValue = category ?? (this.category || undefined)
        if (categoryValue !== undefined) params.category = categoryValue
        if (tags !== undefined) params.tags = tags
        await importAsset(params)
        await this.load()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.importing = false
      }
    },
    async remove(assetId: string): Promise<boolean> {
      this.busyId = assetId
      try {
        await deleteAsset(assetId)
        this.items = this.items.filter((a) => a.id !== assetId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busyId = null
      }
    },
    async assign(assetId: string, projectId: string, targetRel: string): Promise<boolean> {
      this.busyId = assetId
      try {
        await assignAsset({ assetId, projectId, targetRel })
        await this.load()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busyId = null
      }
    },
    selectProject(projectId: string | null): void {
      this.selectedProjectId = projectId
    },
  },
})
