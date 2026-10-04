import { invokeCommand } from './ipc'
import type { LegacyTaskPayload } from '@/types/commands'
import type {
  AccountSummary,
  AntaresInstance,
  DashboardSummary,
  JavaInfo,
  PreflightResult,
} from '@/types/instances'

/**
 * M6 — Core user flows. instances (Batch 05) + accounts/java/versions/
 * dashboard (Batch 06) + play preflight/launch (Batch 07a/07b) đã native
 * (qua composition root `antares-app`, không đụng sidecar) + install
 * (Batch 07c — play_install / auto-install khi Play).
 */

export async function listInstances(): Promise<AntaresInstance[]> {
  const data = await invokeCommand('instances_list')
  return data.instances
}

export async function selectInstance(instanceId: string): Promise<string> {
  const data = await invokeCommand('instances_select', { instanceId })
  return data.selected
}

export async function createInstance(params: {
  name: string
  minecraftVersion: string
  loader?: string
  memoryMaxMb?: number
}): Promise<AntaresInstance> {
  const data = await invokeCommand('instances_create', params)
  return data.instance
}

export async function listAccounts(): Promise<AccountSummary[]> {
  const data = await invokeCommand('accounts_list')
  return data.accounts
}

export async function selectAccount(accountId: string): Promise<string> {
  const data = await invokeCommand('accounts_select', { accountId })
  return data.selected
}

export async function listJavas(): Promise<JavaInfo[]> {
  const data = await invokeCommand('java_list')
  return data.javas
}

export async function listVersions(loader = 'vanilla'): Promise<string[]> {
  const data = await invokeCommand('versions_list', { loader })
  return data.versions.map((v) => (typeof v === 'string' ? v : v.id))
}

export async function runPreflight(instanceId: string): Promise<PreflightResult> {
  return invokeCommand('play_preflight', { instanceId })
}

export async function launchInstance(instanceId: string): Promise<LegacyTaskPayload> {
  const data = await invokeCommand('play_launch', { instanceId })
  return {
    id: data.taskId,
    type: 'LAUNCH',
    owner: `instance:${instanceId}`,
    state: 'running',
    progress: 0,
    message: '',
  }
}

/**
 * Batch 07c — `play_install`: cài/repair instance không launch (native-only).
 * Contract `{taskId}` như launch — task `INSTALL` chạy nền, UI poll task
 * (play_launch cũng tự cài khi version chưa cài — parity orchestrator).
 */
export async function installInstance(instanceId: string): Promise<LegacyTaskPayload> {
  const data = await invokeCommand('play_install', { instanceId })
  return {
    id: data.taskId,
    type: 'INSTALL',
    owner: `instance:${instanceId}`,
    state: 'running',
    progress: 0,
    message: '',
  }
}

export async function getDashboardSummary(): Promise<DashboardSummary> {
  return invokeCommand('dashboard_summary')
}
