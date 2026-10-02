import { defineStore } from 'pinia'

export type NotificationSeverity = 'info' | 'success' | 'warning' | 'error'

export interface AntaresNotification {
  id: string
  severity: NotificationSeverity
  timestamp: number
  title: string
  message: string
  source: string
  read: boolean
  count: number
}

const SPAM_GUARD_WINDOW_MS = 10_000
const MAX_HISTORY = 500

// Clock bọc ngoài để test override được
export const notificationsClock = {
  now(): number {
    return Date.now()
  },
}

let seq = 0
function nextId(): string {
  seq += 1
  return `notif_${seq}`
}

function fingerprint(title: string, source: string): string {
  return `${source}::${title}`
}

export const useNotificationsStore = defineStore('notifications', {
  state: () => ({
    items: [] as AntaresNotification[],
  }),
  getters: {
    unreadCount(state): number {
      return state.items.filter((n) => !n.read).length
    },
  },
  actions: {
    push(severity: NotificationSeverity, title: string, message: string, source = 'app'): void {
      const now = notificationsClock.now()
      const fp = fingerprint(title, source)
      const last = this.items[0]
      if (
        last !== undefined &&
        fingerprint(last.title, last.source) === fp &&
        now - last.timestamp < SPAM_GUARD_WINDOW_MS
      ) {
        last.count += 1
        last.timestamp = now
        return
      }
      this.items.unshift({
        id: nextId(),
        severity,
        timestamp: now,
        title,
        message,
        source,
        read: false,
        count: 1,
      })
      if (this.items.length > MAX_HISTORY) this.items.length = MAX_HISTORY
    },
    markRead(id: string): void {
      const item = this.items.find((n) => n.id === id)
      if (item) item.read = true
    },
    clear(): void {
      this.items = []
    },
  },
})
