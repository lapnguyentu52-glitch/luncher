import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { clearBrowserFallbacks, registerBrowserFallback } from '@/services/ipc'
import { useInstancesStore } from '@/stores/instances.store'
import type { AntaresInstance } from '@/types/instances'

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

describe('instances store load phase (§68 offline)', () => {
  let store: ReturnType<typeof useInstancesStore>

  beforeEach(() => {
    setActivePinia(createPinia())
    clearBrowserFallbacks()
    store = useInstancesStore()
  })

  it('backend ok → ready với items', async () => {
    registerBrowserFallback('instances_list', () => ({
      ok: true,
      data: { instances: [demoInstance] },
      warnings: [],
    }))
    await store.load()
    expect(store.phase).toBe('ready')
    expect(store.items).toHaveLength(1)
  })

  // Regression: trước đây IpcError bị toAntaresError bọc lại thành APP_INTERNAL
  // → phase luôn là 'error' dù backend trả đúng IPC_SESSION_STALE (bridge chưa chạy).
  it('bridge chưa chạy (IPC_SESSION_STALE) → offline, không phải error', async () => {
    registerBrowserFallback('instances_list', () => ({
      ok: false,
      error: { code: 'IPC_SESSION_STALE', message: 'bridge not running', retryable: true },
      warnings: [],
    }))
    await store.load()
    expect(store.phase).toBe('offline')
    expect(store.errorCode).toBe('IPC_SESSION_STALE')
    expect(store.errorMessage).toBe('bridge not running')
  })

  it('sidecar không tìm thấy (IPC_SIDECAR_NOT_FOUND) → offline', async () => {
    registerBrowserFallback('instances_list', () => ({
      ok: false,
      error: { code: 'IPC_SIDECAR_NOT_FOUND', message: 'no sidecar', retryable: false },
      warnings: [],
    }))
    await store.load()
    expect(store.phase).toBe('offline')
    expect(store.errorCode).toBe('IPC_SIDECAR_NOT_FOUND')
  })

  it('lỗi khác → error state', async () => {
    registerBrowserFallback('instances_list', () => ({
      ok: false,
      error: { code: 'METHOD_NOT_FOUND', message: 'no.such', retryable: false },
      warnings: [],
    }))
    await store.load()
    expect(store.phase).toBe('error')
    expect(store.errorCode).toBe('METHOD_NOT_FOUND')
  })
})
