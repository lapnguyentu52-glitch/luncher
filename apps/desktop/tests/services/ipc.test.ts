import { afterEach, describe, expect, it } from 'vitest'

import {
  clearBrowserFallbacks,
  invokeCommand,
  IpcError,
  registerBrowserFallback,
  requestCommand,
  toAntaresError,
} from '@/services/ipc'
import { errorDomain, ErrorCodes, isEventEnvelope } from '@/types/protocol'

describe('ipc browser fallback (dev/test ngoài Tauri)', () => {
  afterEach(() => {
    clearBrowserFallbacks()
  })

  it('fallback đã đăng ký → invokeCommand trả data', async () => {
    registerBrowserFallback('app_ping', () => ({
      ok: true,
      data: { pong: true, tsMs: 123 },
      warnings: [],
    }))
    const payload = await invokeCommand('app_ping')
    expect(payload.pong).toBe(true)
    expect(payload.tsMs).toBe(123)
  })

  it('không có fallback → trả error envelope IPC_SESSION_STALE', async () => {
    const res = await requestCommand('app_ping')
    expect(res.ok).toBe(false)
    expect(res.error?.code).toBe('IPC_SESSION_STALE')
  })

  it('fallback trả error envelope → invokeCommand throw IpcError', async () => {
    registerBrowserFallback('app_version', () => ({
      ok: false,
      error: { code: ErrorCodes.AppNotReady, message: 'booting', retryable: true },
      warnings: [],
    }))
    await expect(invokeCommand('app_version')).rejects.toMatchObject({
      name: 'IpcError',
      antares: { code: 'APP_NOT_READY', retryable: true },
    })
  })

  it('fallback trả raw data (không envelope) → defensive wrap', async () => {
    registerBrowserFallback('app_storage_mode', () => 'portable')
    const mode = await invokeCommand('app_storage_mode')
    expect(mode).toBe('portable')
  })
})

describe('toAntaresError', () => {
  it('giữ nguyên AntaresError shape nếu đã đúng', () => {
    const err = toAntaresError({ code: 'JAVA_NOT_FOUND', message: 'no java', retryable: false })
    expect(err).toEqual({ code: 'JAVA_NOT_FOUND', message: 'no java', retryable: false })
  })

  it('Error thường → APP_INTERNAL', () => {
    const err = toAntaresError(new Error('boom'))
    expect(err.code).toBe('APP_INTERNAL')
    expect(err.message).toBe('boom')
  })

  it('giá trị lạ → string hoá an toàn', () => {
    const err = toAntaresError(42)
    expect(err.code).toBe('APP_INTERNAL')
    expect(err.message).toBe('42')
  })

  // Regression: IpcError bị bọc lại thành APP_INTERNAL → store không phân nhánh
  // được offline §68 → mọi tab hiện state error thay vì "Bridge chưa chạy".
  it('IpcError → giữ nguyên antares code gốc (offline §68 nhận diện được)', () => {
    const ipc = new IpcError({ code: 'IPC_SESSION_STALE', message: 'bridge not running', retryable: true })
    expect(toAntaresError(ipc)).toEqual({
      code: 'IPC_SESSION_STALE',
      message: 'bridge not running',
      retryable: true,
    })
  })
})

describe('IpcError', () => {
  it('expose antares error để UI render theo code', () => {
    const ipc = new IpcError({ code: 'INSTANCE_LOCKED', message: 'busy', retryable: true, action: 'OPEN_INSTANCE' })
    expect(ipc.antares.code).toBe('INSTANCE_LOCKED')
    expect(ipc.antares.action).toBe('OPEN_INSTANCE')
    expect(ipc.message).toContain('INSTANCE_LOCKED')
  })
})

describe('protocol catalog', () => {
  it('errorDomain trích prefix trước underscore', () => {
    expect(errorDomain('JAVA_NOT_FOUND')).toBe('JAVA')
    expect(errorDomain('weird')).toBe('weird')
  })

  it('isEventEnvelope chặn payload thiếu trường', () => {
    const valid = {
      id: 'e1',
      schema: 1,
      topic: 'app',
      qos: 'latest',
      name: 'n',
      timestampMs: 1,
      payload: {},
    }
    expect(isEventEnvelope(valid)).toBe(true)
    expect(isEventEnvelope({ ...valid, id: 42 })).toBe(false)
    expect(isEventEnvelope({ ...valid, payload: undefined })).toBe(false)
    expect(isEventEnvelope(null)).toBe(false)
  })
})
