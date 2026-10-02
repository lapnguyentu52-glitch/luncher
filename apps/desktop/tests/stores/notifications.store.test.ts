import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { notificationsClock, useNotificationsStore } from '@/stores/notifications.store'

describe('notifications store test', () => {
  let store: ReturnType<typeof useNotificationsStore>
  let fakeNow: number

  beforeEach(() => {
    setActivePinia(createPinia())
    fakeNow = 1_000_000
    notificationsClock.now = () => fakeNow
    store = useNotificationsStore()
    store.clear()
  })

  it('push tạo notification mới với count=1', () => {
    store.push('info', 'Hello', 'First message', 'test')
    expect(store.items).toHaveLength(1)
    expect(store.items[0]?.count).toBe(1)
  })

  it('spam guard: cùng title+source trong 10s thì tăng count, không push mới', () => {
    store.push('info', 'Hello', 'First message', 'test')
    store.push('info', 'Hello', 'Second message', 'test')
    expect(store.items).toHaveLength(1)
    expect(store.items[0]?.count).toBe(2)
  })

  it('sau 10s thì push như notification mới', () => {
    store.push('info', 'Hello', 'A', 'test')
    fakeNow += 11_000
    store.push('info', 'Hello', 'B', 'test')
    expect(store.items).toHaveLength(2)
  })

  it('markRead cập nhật unreadCount', () => {
    store.push('info', 'Hello', 'A', 'test')
    expect(store.unreadCount).toBe(1)
    const id = store.items[0]?.id
    if (id !== undefined) store.markRead(id)
    expect(store.unreadCount).toBe(0)
  })

  it('clear xoá toàn bộ', () => {
    store.push('info', 'Hello', 'A', 'test')
    store.clear()
    expect(store.items).toHaveLength(0)
  })
})
