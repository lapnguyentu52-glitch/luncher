/**
 * B8b — Resource Studio pack lifecycle (§33/§126–§128/§173).
 * Mirror services/resources/service.py + builder/validator/installer/layering payloads.
 */

/** §124 — Resource project (mirror project.json). */
export interface ResourceProjectFull {
  id: string
  name: string
  description?: string
  minecraft: { version: string }
  template?: string
  buildCount?: number
  createdAt: number
  updatedAt: number
}

/** §10.2 — wizard metadata cho create-pack wizard. */
export interface ResourceWizardInfo {
  templates: Array<{
    id: string
    labelKey: string
    descKey: string
    modules: Record<string, boolean>
  }>
  versions: string[]
  defaultVersion: string
}

/** §127 — validation finding {code, severity, path, detail}. */
export interface ValidationFinding {
  code: string
  severity: 'ERROR' | 'WARNING' | string
  path: string
  detail: string
}

export interface ValidationResult {
  ok: boolean
  findings: ValidationFinding[]
  packFormat: number
}

/** §126 — build manifest (mirror builder.py build()). */
export interface BuildManifest {
  project: string
  name: string
  mcVersion: string
  packFormat: number
  packMetaMode?: string
  file: string
  sha256: string
  bytes: number
  files: number
  warnings: ValidationFinding[]
  builtAt: number
}

/** Build history entry (list_builds — không kèm fileHashes). */
export type BuildEntry = Omit<BuildManifest, 'fileHashes'>

/** §75 — install result (mirror installer.py install_zip). */
export interface PackInstallResult {
  file: string
  bytes: number
  sha256: string
  backup?: string | null
  installedAt: number
}

/** Installed pack trong resourcepacks/ của instance. */
export interface InstalledPack {
  file: string
  bytes: number
  mtime?: number
}

/** §173 — effective preview: path → pack thắng + conflicts. */
export interface LayerPreview {
  order: string[]
  assets: Record<string, { pack: string; priority: number }>
  conflicts: Array<{ path: string; packs: string[] }>
}

export interface LayerState {
  order: string[]
  preview: LayerPreview
}
