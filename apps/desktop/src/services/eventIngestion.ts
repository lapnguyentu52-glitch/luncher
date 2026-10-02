import { EventQos, EVENT_SCHEMA_VERSION, isEventEnvelope } from '@/types/protocol'
import type { EventEnvelope, EventQos as Qos } from '@/types/protocol'

/**
 * §93 — UI Event Ingestion Pipeline.
 *
 * Rust events → multiplexer → QoS policy → per-topic buffer → frame scheduler → subscribers.
 *
 * Mục tiêu: FPS 60 samples/s vẫn chỉ đẩy ~8 updates/s vào Vue. Raw event rate
 * không được quyết định UI update rate.
 */

export type EventSubscriber = (envelopes: EventEnvelope[]) => void

interface PendingState {
  latest: Map<string, EventEnvelope>
  coalesce: Map<string, EventEnvelope>
  batched: EventEnvelope[]
  lossless: EventEnvelope[]
}

function createPending(): PendingState {
  return {
    latest: new Map(),
    coalesce: new Map(),
    batched: [],
    lossless: [],
  }
}

export interface IngestionOptions {
  /** Toggle dev logging cho event inspector (§36). */
  debug?: boolean
}

export class EventIngestion {
  private pending: PendingState = createPending()
  private subscribers = new Set<EventSubscriber>()
  private scheduled = false
  private debug: boolean
  /** Số envelope bị drop/replace bởi QoS — dùng cho developer overlay §80. */
  qosDropped = 0

  constructor(options: IngestionOptions = {}) {
    this.debug = options.debug ?? false
  }

  subscribe(fn: EventSubscriber): () => void {
    this.subscribers.add(fn)
    return () => {
      this.subscribers.delete(fn)
    }
  }

  /** Entry point từ Tauri event listener. Nhận envelope đã validate. */
  ingest(envelope: EventEnvelope): void {
    if (envelope.schema > EVENT_SCHEMA_VERSION) {
      // §31: schema mới hơn UI hiểu → reject an toàn, không crash
      this.qosDropped += 1
      return
    }

    switch (envelope.qos) {
      case EventQos.Latest:
        this.pending.latest.set(envelope.name, envelope)
        break
      case EventQos.Coalesce:
        this.pending.coalesce.set(coalesceKey(envelope), envelope)
        break
      case EventQos.Batched:
        this.pending.batched.push(envelope)
        break
      case EventQos.Lossless:
        this.pending.lossless.push(envelope)
        break
      default: {
        // qos lạ → coi như batched (an toàn, không mất)
        this.pending.batched.push(envelope)
      }
    }

    if (this.debug) {
      console.debug('[eventIngestion] queued', envelope.name, envelope.qos)
    }
    this.scheduleFlush()
  }

  /** Parse payload thô từ Tauri listener; envelope không hợp lệ bị bỏ qua an toàn (§31). */
  ingestRaw(data: unknown): void {
    if (isEventEnvelope(data)) {
      this.ingest(data)
    } else {
      this.qosDropped += 1
    }
  }

  private scheduleFlush(): void {
    if (this.scheduled) return
    this.scheduled = true
    requestAnimationFrame(() => {
      this.scheduled = false
      this.flush()
    })
  }

  private flush(): void {
    const batch: EventEnvelope[] = [
      ...this.pending.latest.values(),
      ...this.pending.coalesce.values(),
      ...this.pending.batched.splice(0),
      ...this.pending.lossless.splice(0),
    ]
    this.pending.latest.clear()
    this.pending.coalesce.clear()
    if (batch.length === 0 || this.subscribers.size === 0) return
    for (const fn of this.subscribers) {
      fn(batch)
    }
  }
}

function coalesceKey(envelope: EventEnvelope): string {
  const payload = envelope.payload as { key?: unknown } | null
  if (payload && typeof payload === 'object' && typeof payload.key === 'string') {
    return `${envelope.name}::${payload.key}`
  }
  return envelope.name
}

/** QoS gợi ý theo tên topic — dùng ở nơi phát event phía Rust và test. */
export function defaultQosFor(topic: string): Qos {
  if (topic.startsWith('runtime.')) return EventQos.Latest
  if (topic.startsWith('download.')) return EventQos.Coalesce
  if (topic.startsWith('notification.')) return EventQos.Lossless
  return EventQos.Batched
}

/** Singleton cho app runtime. Test tạo instance riêng. */
export const eventIngestion = new EventIngestion()
