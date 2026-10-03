import { invokeCommand, requestCommand } from './ipc'
import type { PingPayload, StorageInfoPayload, TestEventPayload, VersionPayload } from '@/types/commands'
import type { StorageMode } from '@/types/app'

export async function ping(): Promise<PingPayload> {
  return invokeCommand('app_ping')
}

export async function getVersion(): Promise<VersionPayload> {
  return invokeCommand('app_version')
}

export async function getStorageMode(): Promise<StorageMode> {
  return invokeCommand('app_storage_mode')
}

/** F-14 — qua composition root (AppServices): tạo scoped root thiếu + báo cáo. */
export async function getStorageInfo(): Promise<StorageInfoPayload> {
  return invokeCommand('app_storage_info')
}

/** Biến thể request (không throw) cho chỗ UI muốn hiện error inline. */
export async function tryPing(): Promise<{ ok: boolean; tsMs?: number; message?: string }> {
  const res = await requestCommand('app_ping')
  if (res.ok && res.data !== undefined) {
    return { ok: true, tsMs: res.data.tsMs }
  }
  return { ok: false, message: res.error?.message ?? 'unknown error' }
}

export async function testEvent(): Promise<TestEventPayload> {
  return invokeCommand('app_test_event')
}
