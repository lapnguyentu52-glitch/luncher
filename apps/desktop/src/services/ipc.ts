import type { CommandName, CommandSchema } from '@/types/commands'
import type { AntaresError, AntaresResponse } from '@/types/protocol'

/**
 * Typed IPC — điểm duy nhất của UI chạm vào backend (§1.2: UI ≠ filesystem/process).
 *
 * Trong Tauri: gọi invoke thật.
 * Ngoài Tauri (vitest / `pnpm dev` trong browser): dùng browser fallback để
 * dev UI không cần backend — nhưng đánh dấu rõ isTauri = false.
 */

export const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

type BrowserFallback = (command: string, args: InvokeArgs) => unknown

const fallbacks = new Map<CommandName, BrowserFallback>()

export type { InvokeArgs }

/** Đăng ký mock cho một command khi chạy ngoài Tauri (dev/test only). */
export function registerBrowserFallback(command: CommandName, fn: BrowserFallback): void {
  fallbacks.set(command, fn)
}

export function clearBrowserFallbacks(): void {
  fallbacks.clear()
}

export class IpcError extends Error {
  readonly antares: AntaresError

  constructor(error: AntaresError) {
    super(`${error.code}: ${error.message}`)
    this.name = 'IpcError'
    this.antares = error
  }
}

/** Normalise một lỗi bất kỳ (throw từ invoke, network, ...) thành AntaresError. */
export function toAntaresError(err: unknown): AntaresError {
  // IpcError đã bọc AntaresError thật từ backend — giữ nguyên code để store
  // phân nhánh offline §68 (IPC_SESSION_STALE/IPC_SIDECAR_NOT_FOUND), nếu bọc
  // lại thành APP_INTERNAL thì mọi tab luôn rơi vào state error thay vì offline.
  if (err instanceof IpcError) return err.antares
  if (typeof err === 'object' && err !== null && 'code' in err && 'message' in err) {
    const candidate = err as Partial<AntaresError>
    if (typeof candidate.code === 'string' && typeof candidate.message === 'string') {
      const normalized: AntaresError = {
        code: candidate.code,
        message: candidate.message,
        retryable: candidate.retryable ?? false,
      }
      if (candidate.action !== undefined) normalized.action = candidate.action
      return normalized
    }
  }
  const message = err instanceof Error ? err.message : String(err)
  return { code: 'APP_INTERNAL', message, retryable: false }
}

type InvokeArgs = Record<string, unknown> | undefined

function toInvokeArgs(args: unknown): InvokeArgs {
  if (args === undefined || args === null) return undefined
  if (typeof args === 'object') return args as InvokeArgs
  return undefined
}

async function invokeRaw<C extends CommandName>(
  command: C,
  args?: CommandSchema[C]['request'],
): Promise<AntaresResponse<CommandSchema[C]['response']>> {
  const invokeArgs = toInvokeArgs(args)

  if (!isTauri) {
    const fallback = fallbacks.get(command)
    if (fallback === undefined) {
      return {
        ok: false,
        error: {
          code: 'IPC_SESSION_STALE',
          message: `Backend unavailable outside Tauri (no fallback for "${command}")`,
          retryable: false,
        },
        warnings: ['browser-fallback-missing'],
      }
    }
    const raw = fallback(command, invokeArgs) as unknown
    // Defensive: fallback cũng có thể trả raw data thay vì envelope.
    if (typeof raw === 'object' && raw !== null && 'ok' in raw) {
      return raw as AntaresResponse<CommandSchema[C]['response']>
    }
    return { ok: true, data: raw as CommandSchema[C]['response'], warnings: [] }
  }

  const { invoke } = await import('@tauri-apps/api/core')
  const raw = (await invoke(command, invokeArgs)) as unknown
  // Rust trả envelope đã ok; defensive: nếu backend quên wrap, bọc lại thay vì crash UI.
  if (typeof raw === 'object' && raw !== null && 'ok' in raw) {
    return raw as AntaresResponse<CommandSchema[C]['response']>
  }
  return { ok: true, data: raw as CommandSchema[C]['response'], warnings: [] }
}

/**
 * Gọi command typed. Thành công → trả data. Thất bại → throw IpcError.
 * (UI bắt IpcError và render theo code — không bao giờ thấy raw stack trace, §96.)
 */
export async function invokeCommand<C extends CommandName>(
  command: C,
  args?: CommandSchema[C]['request'],
): Promise<CommandSchema[C]['response']> {
  const response = await invokeRaw(command, args)
  if (!response.ok || response.data === undefined) {
    const error =
      response.error ??
      ({ code: 'APP_INTERNAL', message: 'Empty response', retryable: false } satisfies AntaresError)
    throw new IpcError(error)
  }
  return response.data
}

/** Biến thể: không throw — trả toàn bộ envelope để UI tự xử lý warnings/error. */
export async function requestCommand<C extends CommandName>(
  command: C,
  args?: CommandSchema[C]['request'],
): Promise<AntaresResponse<CommandSchema[C]['response']>> {
  try {
    return await invokeRaw(command, args)
  } catch (err) {
    return { ok: false, error: toAntaresError(err), warnings: [] }
  }
}
