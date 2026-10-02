import { callLegacy } from './legacyCommands'
import type {
  BuildEntry,
  BuildManifest,
  InstalledPack,
  LayerState,
  PackInstallResult,
  ResourceProjectFull,
  ResourceWizardInfo,
  ValidationResult,
} from '@/types/resources'

/**
 * B8b — Resource Studio pack lifecycle qua legacy bridge.
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

export async function getWizardInfo(): Promise<ResourceWizardInfo> {
  return callLegacy<ResourceWizardInfo>('resource.wizard_info')
}

export async function createResourceProject(params: {
  name: string
  mcVersion: string
  template?: string
  description?: string
}): Promise<ResourceProjectFull> {
  const data = await callLegacy<{ project: ResourceProjectFull }>('resource.create', params)
  return data.project
}

export async function updateResourceProject(
  projectId: string,
  patch: Partial<Pick<ResourceProjectFull, 'name' | 'description' | 'template'>>,
): Promise<ResourceProjectFull> {
  const data = await callLegacy<{ project: ResourceProjectFull }>('resource.update', {
    projectId,
    patch,
  })
  return data.project
}

export async function deleteResourceProject(projectId: string): Promise<void> {
  await callLegacy<{ deleted: boolean }>('resource.delete', { projectId, confirm: true })
}

export async function generateProject(projectId: string): Promise<{ files: number; packFormat: number }> {
  const data = await callLegacy<{ generated: { files: number; mcVersion: string; packFormat: number } }>(
    'resource.generate',
    { projectId },
  )
  return { files: data.generated.files, packFormat: data.generated.packFormat }
}

/** §127 — findings list; FAIL chỉ khi có ERROR (WARNING không chặn). */
export async function validateProject(projectId: string): Promise<ValidationResult> {
  return callLegacy<ValidationResult>('resource.validate', { projectId })
}

/** §126 — sync build: validate → ZIP + manifest. ERROR → IpcError VALIDATION_FAILED. */
export async function buildProject(projectId: string): Promise<BuildManifest> {
  const data = await callLegacy<{ manifest: BuildManifest }>('resource.build', { projectId })
  return data.manifest
}

export async function listBuilds(projectId: string): Promise<BuildEntry[]> {
  const data = await callLegacy<{ builds: BuildEntry[] }>('resource.builds', { projectId })
  return data.builds
}

/** §33/§75 — build (nếu chưa có) rồi install ZIP vào instance resourcepacks. */
export async function installPack(params: {
  projectId: string
  instanceId: string
  overwrite?: boolean
}): Promise<PackInstallResult> {
  const data = await callLegacy<{ installed: PackInstallResult }>('resource.install', params)
  return data.installed
}

export async function listInstalledPacks(instanceId: string): Promise<InstalledPack[]> {
  const data = await callLegacy<{ packs: InstalledPack[] }>('resource.installed', { instanceId })
  return data.packs
}

export async function uninstallPack(instanceId: string, filename: string): Promise<void> {
  await callLegacy<{ removed: boolean }>('resource.uninstall', { instanceId, filename })
}

/** §173 — layer order (thấp→cao) + effective preview/conflicts. */
export async function getLayerState(instanceId: string): Promise<LayerState> {
  return callLegacy<LayerState>('resource.layer.get', { instanceId })
}

export async function setLayerOrder(instanceId: string, order: string[]): Promise<string[]> {
  const data = await callLegacy<{ order: string[] }>('resource.layer.set', { instanceId, order })
  return data.order
}

/** delta âm = lên = tăng priority. */
export async function moveLayerPack(instanceId: string, filename: string, delta: number): Promise<string[]> {
  const data = await callLegacy<{ order: string[] }>('resource.layer.move', {
    instanceId,
    filename,
    delta,
  })
  return data.order
}
