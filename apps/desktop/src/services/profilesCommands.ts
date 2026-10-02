import { callLegacy } from './legacyCommands'
import type {
  ProfileApplyResult,
  ProfileDetail,
  ProfileExport,
  ProfilePlan,
  ProfileSpec,
  ProfileSummary,
} from '@/types/profiles'

/**
 * B6/M7 — Player Profiles qua legacy bridge.
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

export async function listProfiles(): Promise<ProfileSummary[]> {
  const data = await callLegacy<{ profiles: ProfileSummary[] }>('profiles.list')
  return data.profiles
}

export async function getProfile(profileId: string): Promise<ProfileDetail> {
  const data = await callLegacy<{ profile: ProfileDetail }>('profiles.get', { profileId })
  return data.profile
}

export async function createProfile(name: string, spec: ProfileSpec = {}): Promise<ProfileSummary> {
  const data = await callLegacy<{ profile: ProfileSummary }>('profiles.create', { name, spec })
  return data.profile
}

export async function duplicateProfile(profileId: string): Promise<ProfileSummary> {
  const data = await callLegacy<{ profile: ProfileSummary }>('profiles.duplicate', { profileId })
  return data.profile
}

export async function updateProfile(
  profileId: string,
  patch: { name?: string; spec?: ProfileSpec },
): Promise<ProfileSummary> {
  const data = await callLegacy<{ profile: ProfileSummary }>('profiles.update', { profileId, patch })
  return data.profile
}

export async function deleteProfile(profileId: string): Promise<void> {
  await callLegacy<{ deleted: boolean }>('profiles.delete', { profileId, confirm: true })
}

export async function captureProfile(instanceId: string, name?: string): Promise<ProfileSummary> {
  const params: Record<string, unknown> = { instanceId }
  if (name !== undefined) params.name = name
  const data = await callLegacy<{ profile: ProfileSummary }>('profiles.capture', params)
  return data.profile
}

/** §107 — Diff dự kiến, không ghi gì. */
export async function planProfile(profileId: string): Promise<ProfilePlan> {
  const data = await callLegacy<{ plan: ProfilePlan }>('profiles.plan', { profileId })
  return data.plan
}

/** §106 — Apply = patch current state. Không launch game. */
export async function applyProfile(profileId: string): Promise<ProfileApplyResult> {
  const data = await callLegacy<{ applied: ProfileApplyResult }>('profiles.apply', { profileId })
  return data.applied
}

/** Rollback raw options.txt của lần apply gần nhất (§41 — rollback từng chữ cái). */
export async function revertProfile(): Promise<void> {
  await callLegacy<{ reverted: boolean }>('profiles.revert')
}

export async function exportProfile(profileId: string): Promise<ProfileExport> {
  const data = await callLegacy<{ export: ProfileExport }>('profiles.export', { profileId })
  return data.export
}

export async function importProfile(data: ProfileExport): Promise<ProfileSummary> {
  const result = await callLegacy<{ profile: ProfileSummary }>('profiles.import', { data })
  return result.profile
}
