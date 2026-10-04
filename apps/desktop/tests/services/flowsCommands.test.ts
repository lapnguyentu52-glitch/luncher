import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import {
  createInstance,
  getDashboardSummary,
  listAccounts,
  listInstances,
  listJavas,
  listVersions,
  runPreflight,
  selectAccount,
  selectInstance,
} from '@/services/flowsCommands'
import type { AntaresInstance, PreflightResult } from '@/types/instances'

const demoInstance: AntaresInstance = {
  id: 'abc123',
  name: 'Demo',
  minecraftVersion: '1.21.11',
  loader: 'fabric',
  directory: '/tmp/abc123',
  memory: { minMb: 512, maxMb: 2048 },
  jvmArgs: [],
  jvmPreset: 'auto',
  lastPlayedAt: null,
  launchCount: 0,
}

const demoPreflight: PreflightResult = {
  instanceId: 'abc123',
  checks: [
    { id: 'java', label: 'Java ≥ 21', status: 'pass', detail: '1 runtime' },
    { id: 'account', label: 'Account', status: 'warning', detail: 'no account' },
  ],
  blockers: 0,
  warnings: 1,
  canPlay: true,
}

describe('flows commands (M6 smoke)', () => {
  beforeEach(() => {
    clearBrowserFallbacks()
    registerBrowserFallback('legacy_call', (_cmd, args) => {
      const a = args as { method: string; params?: Record<string, unknown> }
      switch (a.method) {
        default:
          return { ok: false, error: { code: 'METHOD_NOT_FOUND', message: a.method }, warnings: [] }
      }
    })
    // Batch 05 — group instances native (không qua legacy_call).
    registerBrowserFallback('instances_list', () => ({
      ok: true,
      data: { instances: [demoInstance] },
      warnings: [],
    }))
    registerBrowserFallback('instances_select', (_cmd, args) => {
      const a = args as { instanceId: string }
      return { ok: true, data: { selected: a.instanceId }, warnings: [] }
    })
    registerBrowserFallback('instances_create', (_cmd, args) => {
      const a = args as { name: string }
      return { ok: true, data: { instance: { ...demoInstance, name: a.name } }, warnings: [] }
    })
    // Batch 06 — accounts/java/versions/dashboard native (không qua legacy_call).
    registerBrowserFallback('accounts_list', () => ({
      ok: true,
      data: { accounts: [{ id: 'a1', displayName: 'Steve', type: 'offline' }] },
      warnings: [],
    }))
    registerBrowserFallback('accounts_select', (_cmd, args) => {
      const a = args as { accountId: string }
      return { ok: true, data: { selected: a.accountId }, warnings: [] }
    })
    registerBrowserFallback('java_list', () => ({
      ok: true,
      data: { javas: [{ path: '/j', exe: '/j/java', major: 21, name: 'jdk21' }] },
      warnings: [],
    }))
    registerBrowserFallback('versions_list', () => ({
      ok: true,
      data: { versions: [{ id: '1.21.11', type: 'release' }] },
      warnings: [],
    }))
    registerBrowserFallback('dashboard_summary', () => ({
      ok: true,
      data: {
        appVersion: '4.0.0',
        instanceCount: 1,
        selectedInstanceId: 'abc123',
        recentInstanceId: 'abc123',
        recentInstanceName: 'Demo',
        account: { id: 'a1', displayName: 'Steve' },
        legacyAvailable: true,
      },
      warnings: [],
    }))
    // Batch 07a — play preflight native (không qua legacy_call).
    registerBrowserFallback('play_preflight', (_cmd, args) => {
      const a = args as { instanceId: string }
      return {
        ok: true,
        data: { ...demoPreflight, instanceId: a.instanceId },
        warnings: [],
      }
    })
    // Batch 07b — play launch native (contract {taskId} như sidecar).
    registerBrowserFallback('play_launch', () => ({
      ok: true,
      data: { taskId: 'task-99' },
      warnings: [],
    }))
    // Batch 07c — play install native (contract {taskId}, task INSTALL chạy nền).
    registerBrowserFallback('play_install', () => ({
      ok: true,
      data: { taskId: 'task-77' },
      warnings: [],
    }))
  })
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('listInstances trả AntaresInstance[]', async () => {
    const items = await listInstances()
    expect(items).toHaveLength(1)
    expect(items[0]?.minecraftVersion).toBe('1.21.11')
  })

  it('selectInstance echo selected id', async () => {
    const selected = await selectInstance('abc123')
    expect(selected).toBe('abc123')
  })

  it('createInstance giữ nguyên shape', async () => {
    const inst = await createInstance({ name: 'New', minecraftVersion: '1.21.11' })
    expect(inst.name).toBe('New')
  })

  it('listJavas trả major', async () => {
    const javas = await listJavas()
    expect(javas[0]?.major).toBe(21)
  })

  it('listAccounts trả accounts đã strip secret (Batch 06 native)', async () => {
    const accounts = await listAccounts()
    expect(accounts).toHaveLength(1)
    expect(accounts[0]?.displayName).toBe('Steve')
  })

  it('selectAccount echo account id (Batch 06 native)', async () => {
    const selected = await selectAccount('a1')
    expect(selected).toBe('a1')
  })

  it('listVersions map id từ object (Batch 06 native)', async () => {
    const versions = await listVersions()
    expect(versions).toEqual(['1.21.11'])
  })

  it('runPreflight trả canPlay + checks', async () => {
    const result = await runPreflight('abc123')
    expect(result.canPlay).toBe(true)
    expect(result.checks.length).toBeGreaterThan(0)
  })

  it('launchInstance map taskId → LegacyTaskPayload', async () => {
    const { launchInstance } = await import('@/services/flowsCommands')
    const task = await launchInstance('abc123')
    expect(task.id).toBe('task-99')
    expect(task.owner).toBe('instance:abc123')
    expect(task.state).toBe('running')
  })

  it('installInstance map taskId → LegacyTaskPayload type INSTALL (07c)', async () => {
    const { installInstance } = await import('@/services/flowsCommands')
    const task = await installInstance('abc123')
    expect(task.id).toBe('task-77')
    expect(task.type).toBe('INSTALL')
    expect(task.owner).toBe('instance:abc123')
    expect(task.state).toBe('running')
  })

  it('getDashboardSummary trả summary đầy đủ', async () => {
    const summary = await getDashboardSummary()
    expect(summary.instanceCount).toBe(1)
    expect(summary.account?.displayName).toBe('Steve')
  })

  it('method lạ → IpcError METHOD_NOT_FOUND', async () => {
    const { callLegacy } = await import('@/services/legacyCommands')
    await expect(callLegacy('unknown.method')).rejects.toMatchObject({
      antares: { code: 'METHOD_NOT_FOUND' },
    })
  })
})
