import { callLegacy } from './legacyCommands'
import type { AssetEntry, AssetWithPreview, ResourceProject } from '@/types/assets'

/**
 * B8a — Asset Library + Resource Studio qua legacy bridge.
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

export interface AssetListParams {
  query?: string
  category?: string
  tag?: string
  sort?: 'newest' | 'oldest' | 'name' | 'size'
}

export async function listAssets(params: AssetListParams = {}): Promise<AssetEntry[]> {
  const data = await callLegacy<{ assets: AssetEntry[] }>('asset.list', params)
  return data.assets
}

/** Import PNG (base64 từ FileReader) — service validate signature/dims/size §12.2. */
export async function importAsset(params: {
  dataB64: string
  name?: string
  category?: string
  tags?: string[]
}): Promise<AssetEntry> {
  const data = await callLegacy<{ asset: AssetEntry }>('asset.import', params)
  return data.asset
}

export async function getAsset(assetId: string): Promise<AssetWithPreview> {
  return callLegacy<AssetWithPreview>('asset.get', { assetId })
}

export async function deleteAsset(assetId: string): Promise<void> {
  await callLegacy<{ deleted: boolean }>('asset.delete', { assetId })
}

/** Gán asset vào project path — target phải dưới assets/<ns>/textures/ (service validate). */
export async function assignAsset(params: {
  projectId: string
  assetId: string
  targetRel: string
}): Promise<{ path: string; asset: string; sha256: string }> {
  const data = await callLegacy<{ assigned: { path: string; asset: string; sha256: string } }>(
    'asset.assign',
    params,
  )
  return data.assigned
}

export async function listResourceProjects(): Promise<ResourceProject[]> {
  const data = await callLegacy<{ projects: ResourceProject[] }>('resource.list')
  return data.projects
}

export async function getResourceProject(projectId: string): Promise<ResourceProject> {
  const data = await callLegacy<{ project: ResourceProject }>('resource.get', { projectId })
  return data.project
}

/** File → base64 (không prefix data URI) cho asset.import. */
export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = String(reader.result ?? '')
      const comma = result.indexOf(',')
      resolve(comma >= 0 ? result.slice(comma + 1) : result)
    }
    reader.onerror = () => reject(new Error(`Failed to read ${file.name}`))
    reader.readAsDataURL(file)
  })
}
