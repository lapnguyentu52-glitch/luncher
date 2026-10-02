/** Mirror của crates/antares-bridge/src/protocol.rs VersionInfo (§234). */
export interface LegacyVersionInfo {
  protocol: number
  service: string
  serviceVersion: string
  python: string
  legacyVersion?: string
}

/** Mirror của commands/legacy.rs LegacyStatusPayload. */
export interface LegacyStatusPayload {
  running: boolean
  program?: string
  version?: LegacyVersionInfo
  crashCount: number
}

/** Methods chuẩn của sidecar (§43) — không hardcode string lẻ trong UI. */
export const LegacyMethods = {
  Ping: 'health.ping',
  Version: 'health.version',
  Shutdown: 'health.shutdown',
  Echo: 'app.echo',
  StorageRoot: 'app.storage_root',
} as const

export const LEGACY_PROTOCOL_VERSION = 1
