import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  applyProfile,
  deleteProfile,
  listProfiles,
  planProfile,
  revertProfile,
} from '@/services/profilesCommands'
import type { ProfilePlan, ProfileSummary } from '@/types/profiles'

const demoProfile: ProfileSummary = {
  id: 'prof-20260928-000000-abcdef',
  name: 'Competitive',
  createdAt: 1759000000,
  updatedAt: 1759000000,
  spec: { instance: { id: 'i1' }, jvm: { memory: { maxMb: 4096 } } },
  active: false,
  lastAppliedAt: null,
}

const demoPlan: ProfilePlan = {
  profileId: demoProfile.id,
  profileName: 'Competitive',
  hasChanges: true,
  changes: {
    account: null,
    instance: { field: 'id', before: 'i1', after: 'i2', afterName: 'Skyblock' },
    jvm: [],
    game: [],
    launch: [],
  },
}

describe('profiles commands (B6 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        case 'profiles.list':
          return { ok: true, data: { profiles: [demoProfile] }, warnings: [] }
        case 'profiles.plan':
          return { ok: true, data: { plan: { ...demoPlan, profileId: String(a.params?.profileId) } }, warnings: [] }
        case 'profiles.apply':
          return {
            ok: true,
            data: { applied: { profileId: String(a.params?.profileId), plan: demoPlan, appliedAt: 1759000100 } },
            warnings: [],
          }
        case 'profiles.delete':
          return { ok: true, data: { deleted: true }, warnings: [] }
        case 'profiles.revert':
          return { ok: true, data: { reverted: true }, warnings: [] }
        default:
          return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method }, warnings: [] }
      }
    })
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('listProfiles trả typed summaries', async () => {
    const profiles = await listProfiles()
    expect(profiles).toHaveLength(1)
    expect(profiles[0]?.id).toBe(demoProfile.id)
    expect(profiles[0]?.active).toBe(false)
  })

  it('planProfile trả diff §107', async () => {
    const plan = await planProfile('p1')
    expect(plan.hasChanges).toBe(true)
    expect(plan.changes.instance?.after).toBe('i2')
  })

  it('applyProfile bọc result trong applied', async () => {
    const result = await applyProfile('p1')
    expect(result.profileId).toBe('p1')
    expect(result.appliedAt).toBe(1759000100)
  })

  it('deleteProfile gửi confirm:true (bắt buộc §107 service)', async () => {
    let captured: unknown
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      captured = a.params
      return { ok: true, data: { deleted: true }, warnings: [] }
    })
    await deleteProfile('p1')
    expect((captured as Record<string, unknown>)?.confirm).toBe(true)
  })

  it('revertProfile ok', async () => {
    await expect(revertProfile()).resolves.toBeUndefined()
  })

  it('method không tồn tại → throw IpcError code', async () => {
    await expect(
      listProfiles().then(() => undefined),
    ).resolves.toBeUndefined()
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string }
      return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method }, warnings: [] }
    })
    await expect(listProfiles()).rejects.toMatchObject({ antares: { code: 'METHOD_NOT_FOUND' } })
  })
})
