/**
 * B6/M7 — Player Profiles (§24/§40/§106/§107).
 * Mirror services/profiles/service.py — payload list đã strip `gameRaw`.
 */

export type ProfileSpec = {
  account?: { id: string }
  instance?: { id: string }
  jvm?: { memory?: unknown; jvmPreset?: string; jvmArgs?: string[] }
  game?: Record<string, unknown>
  launch?: Record<string, string | number | boolean>
}

export interface ProfileSummary {
  id: string
  name: string
  createdAt: number
  updatedAt: number
  spec: ProfileSpec
  active: boolean
  lastAppliedAt: number | null
}

export interface ProfileDetail extends ProfileSummary {
  gameRaw?: string
}

/** §107 — một dòng diff của plan. */
export interface ProfileChange {
  field: string
  before: unknown
  after: unknown
  afterName?: string
}

export interface ProfilePlan {
  profileId: string
  profileName: string
  hasChanges: boolean
  changes: {
    account: { before: string | null; after: string | null } | null
    instance: ProfileChange | null
    jvm: ProfileChange[]
    game: ProfileChange[]
    launch: ProfileChange[]
  }
}

export interface ProfileApplyResult {
  profileId: string
  plan: ProfilePlan
  appliedAt: number
}

/** antares-profile export format (mục 40). */
export interface ProfileExport {
  format: 'antares-profile'
  version: number
  name: string
  spec: ProfileSpec
}
