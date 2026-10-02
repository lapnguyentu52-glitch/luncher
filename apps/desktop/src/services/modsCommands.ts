import { callLegacy } from './legacyCommands'
import type {
  InstalledMod,
  ModDetail,
  ModsFixResult,
  ModsHealth,
  ModsScanResult,
  ModSearchHit,
  ModpackTask,
  MrpackInfo,
  QuarantineEntry,
} from '@/types/mods'

/**
 * B7/M7 — Mod Manager qua legacy bridge.
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

export async function listInstalledMods(instanceId: string): Promise<InstalledMod[]> {
  const data = await callLegacy<{ mods: InstalledMod[] }>('mods.list', { instanceId })
  return data.mods
}

export async function searchMods(params: {
  query: string
  loader: string
  mcVersion: string
}): Promise<ModSearchHit[]> {
  const data = await callLegacy<{ hits: ModSearchHit[] }>('mods.search', params)
  return data.hits
}

export async function installMod(params: {
  projectId: string
  instanceId: string
  loader?: string
  mcVersion?: string
}): Promise<string> {
  const data = await callLegacy<{ filename: string }>('mods.install', params)
  return data.filename
}

export async function removeMod(instanceId: string, filename: string): Promise<void> {
  await callLegacy<{ removed: boolean }>('mods.remove', { instanceId, filename })
}

/** §162 — compatibility/dependency issues cho instance. */
export async function getModsHealth(
  instanceId: string,
  includeOutdated = false,
): Promise<ModsHealth> {
  const data = await callLegacy<{ health: ModsHealth }>('mods.health', {
    instanceId,
    includeOutdated,
  })
  return data.health
}

/** §164 — heuristic scan; DANGEROUS tự quarantine (move, không delete). */
export async function scanMods(instanceId: string): Promise<ModsScanResult> {
  const data = await callLegacy<{ scan: ModsScanResult }>('mods.scan', { instanceId })
  return data.scan
}

/** Auto-fix deps thiếu (vd Fabric API) từ Modrinth — online-only. */
export async function autofixMods(instanceId: string): Promise<ModsFixResult> {
  const data = await callLegacy<{ fix: ModsFixResult }>('mods.autofix', { instanceId })
  return data.fix
}

export async function listQuarantine(): Promise<QuarantineEntry[]> {
  const data = await callLegacy<{ items: QuarantineEntry[] }>('mods.quarantine.list')
  return data.items
}

export async function restoreQuarantined(quarantineFile: string): Promise<string> {
  const data = await callLegacy<{ restored: string }>('mods.quarantine.restore', {
    quarantineFile,
  })
  return data.restored
}

export async function deleteQuarantined(quarantineFile: string): Promise<void> {
  await callLegacy<{ deleted: boolean }>('mods.quarantine.delete', {
    quarantineFile,
    confirm: true,
  })
}

/** §11 — mod detail panel: metadata + depends/recommends/breaks. */
export async function getModDetail(instanceId: string, filename: string): Promise<ModDetail> {
  const data = await callLegacy<{ detail: ModDetail }>('mod.detail', { instanceId, filename })
  return data.detail
}

/** B7b — preview .mrpack trước khi install (name/loader/version/optional files). */
export async function getMrpackInfo(mrpackPath: string): Promise<MrpackInfo> {
  const data = await callLegacy<{ info: MrpackInfo }>('modpack.info', { mrpackPath })
  return data.info
}

/** Install .mrpack async — trả taskId; progress qua pollModpackStatus (§90/§171). */
export async function installMrpack(params: {
  mrpackPath: string
  instanceId: string
  optionalSelected?: string[]
}): Promise<string> {
  const data = await callLegacy<{ taskId: string }>('modpack.install', params)
  return data.taskId
}

/** Poll 1 task — UI cadence 1s (§171: không spam command). */
export async function pollModpackStatus(taskId: string): Promise<ModpackTask> {
  const data = await callLegacy<{ task: ModpackTask }>('modpack.status', { taskId })
  return data.task
}

export async function cancelModpack(taskId: string): Promise<void> {
  await callLegacy<{ cancelled: boolean }>('modpack.cancel', { taskId })
}
