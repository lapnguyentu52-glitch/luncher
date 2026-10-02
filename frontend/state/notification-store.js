/**
 * Notification store — nguồn sự thật phía UI cho Notification Center (spec 3.0 mục 6).
 *
 * - Cập nhật realtime qua event `notification.created` / `notifications.changed`
 *   (backend đẩy, KHÔNG poll).
 * - Nạp dữ liệu ban đầu đúng 1 lần (`seed()`), sau đó chỉ sống bằng event.
 * - Danh sách mới nhất đứng đầu, history giới hạn (mirror policy backend — mục 80).
 */
import { call } from '../app/bridge.js';
import { onBackendEvent } from '../app/events.js';

const MAX_ITEMS = 200;

const items = [];
const listeners = new Set();

let unread = 0;
let seeded = false;
let seeding = null;

/** Gộp nhiều event liên tiếp thành 1 lần notify subscribers. */
function debounce(fn, ms) {
  let timer = null;
  return (...args) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { timer = null; fn(...args); }, ms);
  };
}

const emit = debounce(() => {
  for (const fn of [...listeners]) {
    try { fn(snapshot()); } catch (e) { console.error('[notif-store] listener', e); }
  }
}, 80);

function snapshot() {
  return { items: [...items], unread, total: items.length };
}

function insertTop(item) {
  const idx = items.findIndex((n) => n.id === item.id);
  if (idx >= 0) {
    items[idx] = { ...items[idx], ...item };
  } else {
    items.unshift(item);
    if (items.length > MAX_ITEMS) items.length = MAX_ITEMS;
  }
}

/* ------------------------------------------------------------------ */
/* Event wiring (một lần duy nhất)                                     */
/* ------------------------------------------------------------------ */

let wired = false;

function wire() {
  if (wired) return;
  wired = true;

  onBackendEvent('notification.created', (e) => {
    const n = e.payload || {};
    if (!n.id) return;
    insertTop(n);
    if (!n.read) unread += 1;
    emit();
  });

  onBackendEvent('notifications.changed', () => {
    // read/clear trên backend -> lấy lại số chưa đọc (payload nhẹ, rẻ).
    refreshUnread();
  });
}

async function refreshUnread() {
  const res = await call('notifications_unread_count');
  if (res?.ok) {
    unread = Number(res.data?.unread || 0);
    emit();
  }
}

/* ------------------------------------------------------------------ */
/* Public API                                                          */
/* ------------------------------------------------------------------ */

/** Nạp 1 lần lúc mở app. Các lần sau chỉ sống bằng event. */
export async function seed() {
  wire();
  if (seeded) return snapshot();
  if (seeding) return seeding;
  seeding = (async () => {
    const res = await call('notifications_list', 100);
    if (res?.ok) {
      items.length = 0;
      for (const n of res.data?.notifications || []) items.push(n);
      unread = Number(res.data?.unread || 0);
      seeded = true;
      emit();
    }
    return snapshot();
  })();
  try { return await seeding; } finally { seeding = null; }
}

export function onChange(fn) {
  wire();
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function getUnread() {
  return unread;
}

export function getItems() {
  return items.slice();
}

export function isOpen() {
  return document.getElementById('notif-drawer')?.classList.contains('open') || false;
}

export function markRead(id) {
  return call('notifications_mark_read', id).then((res) => {
    if (res?.ok) {
      if (id) {
        const n = items.find((x) => x.id === id);
        if (n && !n.read) { n.read = true; unread = Math.max(0, unread - 1); }
      } else {
        for (const n of items) n.read = true;
        unread = 0;
      }
      emit();
    }
    return res?.ok || false;
  });
}

export function clear(id) {
  return call('notifications_clear', id).then((res) => {
    if (res?.ok) {
      if (id) {
        const idx = items.findIndex((x) => x.id === id);
        if (idx >= 0) {
          if (!items[idx].read) unread = Math.max(0, unread - 1);
          items.splice(idx, 1);
        }
      } else {
        items.length = 0;
        unread = 0;
      }
      emit();
    }
    return res?.ok || false;
  });
}
