import { defineStore } from 'pinia'

import {
  applyProfile,
  deleteProfile,
  listProfiles,
  planProfile,
  revertProfile,
} from '@/services/profilesCommands'
import { toAntaresError } from '@/services/ipc'
import type { ProfilePlan, ProfileSummary } from '@/types/profiles'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'

interface OfflineCode {
  code: string | null
}

/** §158 — phase machine đầy đủ; offline §68 khi bridge chưa chạy. */
function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

interface ProfilesState {
  phase: LoadPhase
  items: ProfileSummary[]
  plan: ProfilePlan | null
  planLoading: boolean
  applying: boolean
  reverting: boolean
  errorMessage: string | null
  errorCode: string | null
}

export const useProfilesStore = defineStore('profiles', {
  state: (): ProfilesState => ({
    phase: 'idle',
    items: [],
    plan: null,
    planLoading: false,
    applying: false,
    reverting: false,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    activeProfile(state): ProfileSummary | undefined {
      return state.items.find((p) => p.active)
    },
  },
  actions: {
    async load(): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        this.items = await listProfiles()
        this.phase = 'ready'
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    /** §107 — Preview changes trước khi apply. Không ghi gì. */
    async refreshPlan(profileId: string): Promise<void> {
      this.planLoading = true
      this.plan = null
      try {
        this.plan = await planProfile(profileId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.planLoading = false
      }
    },
    /** §99/§106 — apply = mutation có verify ở service; UI chỉ trigger + refresh. */
    async apply(profileId: string): Promise<boolean> {
      this.applying = true
      try {
        await applyProfile(profileId)
        await this.load()
        this.plan = null
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.applying = false
      }
    },
    async remove(profileId: string): Promise<boolean> {
      try {
        await deleteProfile(profileId)
        await this.load()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    /** §167 — Rollback về raw options trước lần apply gần nhất. */
    async revert(): Promise<boolean> {
      this.reverting = true
      try {
        await revertProfile()
        await this.load()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.reverting = false
      }
    },
  },
})

export type { OfflineCode }
