import { callLegacy } from './legacyCommands'
import type {
  CleanupPreview,
  CleanupResult,
  OptimizationApplyResult,
  OptimizationPlan,
  OptimizationScan,
  OptimizationSnapshotInfo,
  PowerStatus,
  SystemOverview,
} from '@/types/optimization'

/**
 * B10 — Game + System optimization qua legacy bridge.
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

// ---- Game optimization (§3/§70) ----

export async function scanOptimization(instanceId?: string): Promise<OptimizationScan> {
  const params: { instanceId?: string } = {}
  if (instanceId) params.instanceId = instanceId
  const data = await callLegacy<{ scan: OptimizationScan }>('optimization.scan', params)
  return data.scan
}

/** Preview diff — không ghi gì. UI bắt buộc plan-first trước apply (§3.1). */
export async function planOptimization(instanceId: string, profileId: string): Promise<OptimizationPlan> {
  const data = await callLegacy<{ plan: OptimizationPlan }>('optimization.plan', {
    instanceId,
    profileId,
  })
  return data.plan
}

/** Snapshot rồi apply — trả snapshot để rollback 1 click (§70). */
export async function applyOptimization(instanceId: string, profileId: string): Promise<OptimizationApplyResult> {
  const data = await callLegacy<{ applied: OptimizationApplyResult }>('optimization.apply', {
    instanceId,
    profileId,
  })
  return data.applied
}

export async function rollbackOptimization(instanceId: string): Promise<void> {
  await callLegacy<{ restored: boolean }>('optimization.rollback', { instanceId })
}

export async function getOptimizationSnapshot(instanceId: string): Promise<OptimizationSnapshotInfo | null> {
  const data = await callLegacy<{ snapshot: OptimizationSnapshotInfo | null }>(
    'optimization.snapshot_info',
    { instanceId },
  )
  return data.snapshot
}

// ---- System optimization (§4/§35–§37) ----

export async function getSystemOverview(): Promise<SystemOverview> {
  const data = await callLegacy<{ overview: SystemOverview }>('system.overview')
  return data.overview
}

export async function scanCleanup(): Promise<CleanupPreview> {
  const data = await callLegacy<{ preview: CleanupPreview }>('system.cleanup.scan')
  return data.preview
}

/** Clean paths chọn — undo-able qua cleanId. */
export async function cleanPaths(paths: string[]): Promise<CleanupResult> {
  const data = await callLegacy<{ clean: CleanupResult }>('system.cleanup.clean', { paths })
  return data.clean
}

export async function undoCleanup(cleanId: string): Promise<CleanupResult> {
  const data = await callLegacy<{ undo: CleanupResult }>('system.cleanup.undo', { cleanId })
  return data.undo
}

export async function emptyTrash(): Promise<number> {
  const data = await callLegacy<{ removed: number }>('system.cleanup.empty_trash')
  return data.removed
}

export async function getPowerStatus(): Promise<PowerStatus> {
  const data = await callLegacy<{ power: PowerStatus }>('system.power.status')
  return data.power
}

export async function setPowerPlan(planId: string): Promise<void> {
  await callLegacy<{ set: boolean }>('system.power.set_plan', { planId })
}
