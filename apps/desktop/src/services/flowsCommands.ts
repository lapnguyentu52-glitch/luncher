import { callLegacy } from './legacyCommands'
import type { LegacyTaskPayload } from '@/types/commands'
import type {
  AccountSummary,
  AntaresInstance,
  DashboardSummary,
  JavaInfo,
  PreflightResult,
} from '@/types/instances'

/**
 * M6 — Core user flows qua legacy bridge.
 * Mỗi helper = 1 sidecar method, typed response. Bridge not running → IpcError.
 */

export async function listInstances(): Promise<AntaresInstance[]> {
  const data = await callLegacy<{ instances: AntaresInstance[] }>('instances.list')
  return data.instances
}

export async function selectInstance(instanceId: string): Promise<string> {
  const data = await callLegacy<{ selected: string }>('instances.select', { instanceId })
  return data.selected
}

export async function createInstance(params: {
  name: string
  minecraftVersion: string
  loader?: string
  memoryMaxMb?: number
}): Promise<AntaresInstance> {
  const data = await callLegacy<{ instance: AntaresInstance }>('instances.create', params)
  return data.instance
}

export async function listAccounts(): Promise<AccountSummary[]> {
  const data = await callLegacy<{ accounts: AccountSummary[] }>('accounts.list')
  return data.accounts
}

export async function selectAccount(accountId: string): Promise<string> {
  const data = await callLegacy<{ selected: string }>('accounts.select', { accountId })
  return data.selected
}

export async function listJavas(): Promise<JavaInfo[]> {
  const data = await callLegacy<{ javas: JavaInfo[] }>('java.list')
  return data.javas
}

export async function listVersions(loader = 'vanilla'): Promise<string[]> {
  const data = await callLegacy<{ versions: Array<string | { id: string }> }>('versions.list', { loader })
  return data.versions.map((v) => (typeof v === 'string' ? v : v.id))
}

export async function runPreflight(instanceId: string): Promise<PreflightResult> {
  return callLegacy<PreflightResult>('play.preflight', { instanceId })
}

export async function launchInstance(instanceId: string): Promise<LegacyTaskPayload> {
  const data = await callLegacy<{ taskId: string }>('play.launch', { instanceId })
  return {
    id: data.taskId,
    type: 'LAUNCH',
    owner: `instance:${instanceId}`,
    state: 'running',
    progress: 0,
    message: '',
  }
}

export async function getDashboardSummary(): Promise<DashboardSummary> {
  return callLegacy<DashboardSummary>('dashboard.summary')
}
