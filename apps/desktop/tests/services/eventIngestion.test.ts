import { describe, expect, it, vi } from 'vitest'

import { defaultQosFor, EventIngestion } from '@/services/eventIngestion'
import { EventQos, EventTopic } from '@/types/protocol'
import type { EventEnvelope } from '@/types/protocol'

function makeEnvelope(overrides: Partial<EventEnvelope> = {}): EventEnvelope {
  return {
    id: 'evt_test',
    schema: 1,
    topic: EventTopic.Telemetry,
    qos: EventQos.Latest,
    name: 'runtime.fps',
    timestampMs: 1_000,
    payload: { fps: 144 },
    ...overrides,
  }
}

/** Kích hoạt requestAnimationFrame đã schedule. */
function flushFrames(): void {
  vi.advanceTimersByTime(16)
}

describe('eventIngestion QoS pipeline (§93)', () => {
  it('LATEST: giữ giá trị mới nhất theo name', () => {
    vi.useFakeTimers()
    const ingestion = new EventIngestion()
    const received: EventEnvelope[][] = []
    ingestion.subscribe((batch) => received.push(batch))

    ingestion.ingest(makeEnvelope({ payload: { fps: 60 } }))
    ingestion.ingest(makeEnvelope({ payload: { fps: 90 } }))
    ingestion.ingest(makeEnvelope({ payload: { fps: 144 } }))
    flushFrames()

    expect(received).toHaveLength(1)
    const flat = received[0] ?? []
    expect(flat).toHaveLength(1)
    expect((flat[0]?.payload as { fps: number }).fps).toBe(144)
    vi.useRealTimers()
  })

  it('COALESCE: gộp theo key trong payload', () => {
    vi.useFakeTimers()
    const ingestion = new EventIngestion()
    const received: EventEnvelope[][] = []
    ingestion.subscribe((batch) => received.push(batch))

    for (const key of ['a', 'a', 'b', 'a']) {
      ingestion.ingest(
        makeEnvelope({
          qos: EventQos.Coalesce,
          name: 'download.progress',
          payload: { key, bytes: 1 },
        }),
      )
    }
    flushFrames()

    const flat = received[0] ?? []
    expect(flat).toHaveLength(2)
    expect(flat.map((e) => (e.payload as { key: string }).key).sort()).toEqual(['a', 'b'])
    vi.useRealTimers()
  })

  it('BATCHED: giữ tất cả, gom một lần flush', () => {
    vi.useFakeTimers()
    const ingestion = new EventIngestion()
    const received: EventEnvelope[][] = []
    ingestion.subscribe((batch) => received.push(batch))

    for (let i = 0; i < 5; i += 1) {
      ingestion.ingest(makeEnvelope({ qos: EventQos.Batched, name: `console.line.${i}` }))
    }
    flushFrames()

    expect(received).toHaveLength(1)
    expect(received[0]).toHaveLength(5)
    vi.useRealTimers()
  })

  it('LOSSLESS: không drop, đẩy đủ toàn bộ', () => {
    vi.useFakeTimers()
    const ingestion = new EventIngestion()
    const received: EventEnvelope[][] = []
    ingestion.subscribe((batch) => received.push(batch))

    for (let i = 0; i < 3; i += 1) {
      ingestion.ingest(makeEnvelope({ qos: EventQos.Lossless, name: `critical.error.${i}` }))
    }
    flushFrames()

    expect(received[0]).toHaveLength(3)
    vi.useRealTimers()
  })

  it('schema mới hơn UI hiểu → reject an toàn, không crash', () => {
    vi.useFakeTimers()
    const ingestion = new EventIngestion()
    const received: EventEnvelope[][] = []
    ingestion.subscribe((batch) => received.push(batch))

    ingestion.ingest(makeEnvelope({ schema: 99 }))
    flushFrames()

    expect(received).toHaveLength(0)
    expect(ingestion.qosDropped).toBe(1)
    vi.useRealTimers()
  })

  it('ingestRaw: payload không hợp lệ bị bỏ qua an toàn (§31)', () => {
    const ingestion = new EventIngestion()
    ingestion.ingestRaw({ random: 'junk' })
    ingestion.ingestRaw(null)
    ingestion.ingestRaw('string')
    expect(ingestion.qosDropped).toBe(3)
  })

  it('không có subscriber → không flush, không crash', () => {
    vi.useFakeTimers()
    const ingestion = new EventIngestion()
    ingestion.ingest(makeEnvelope())
    flushFrames()
    // chỉ cần không throw; subscriber thêm sau vẫn nhận batch mới
    const received: EventEnvelope[][] = []
    ingestion.subscribe((batch) => received.push(batch))
    ingestion.ingest(makeEnvelope({ name: 'second' }))
    flushFrames()
    expect(received).toHaveLength(1)
    vi.useRealTimers()
  })

  it('unsubscribe ngừng nhận event', () => {
    vi.useFakeTimers()
    const ingestion = new EventIngestion()
    const received: EventEnvelope[][] = []
    const unsub = ingestion.subscribe((batch) => received.push(batch))
    unsub()
    ingestion.ingest(makeEnvelope())
    flushFrames()
    expect(received).toHaveLength(0)
    vi.useRealTimers()
  })
})

describe('defaultQosFor', () => {
  it('map topic → QoS policy đúng (§32)', () => {
    expect(defaultQosFor('runtime.fps')).toBe(EventQos.Latest)
    expect(defaultQosFor('download.progress')).toBe(EventQos.Coalesce)
    expect(defaultQosFor('notification.critical')).toBe(EventQos.Lossless)
    expect(defaultQosFor('app.boot')).toBe(EventQos.Batched)
  })
})
