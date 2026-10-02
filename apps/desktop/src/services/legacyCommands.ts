import { invokeCommand } from './ipc'
import type { LegacyStatusPayload, LegacyVersionInfo } from '@/types/legacy'

export async function getLegacyStatus(): Promise<LegacyStatusPayload> {
  return invokeCommand('legacy_status')
}

export async function startLegacy(): Promise<LegacyVersionInfo> {
  return invokeCommand('legacy_start')
}

export async function callLegacy<T = unknown>(
  method: string,
  params?: unknown,
  timeoutMs?: number,
): Promise<T> {
  const args: { method: string; params?: unknown; timeoutMs?: number } = { method }
  if (params !== undefined) args.params = params
  if (timeoutMs !== undefined) args.timeoutMs = timeoutMs
  return invokeCommand('legacy_call', args) as Promise<T>
}

export async function restartLegacy(): Promise<LegacyVersionInfo> {
  return invokeCommand('legacy_restart')
}

export async function shutdownLegacy(): Promise<{ stopped: boolean }> {
  return invokeCommand('legacy_shutdown')
}
