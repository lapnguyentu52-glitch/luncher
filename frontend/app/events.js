/**
 * Events — kênh event từ backend (spec mục 8.2, 15.2).
 *
 * Backend đẩy batch qua `window.__antaresEvent(payload)` (EventBridge gộp +
 * throttle ~12 nhịp/giây). Nếu kênh đẩy im lặng (WebView cũ, evaluate_js lỗi,
 * chạy ngoài pywebview) thì tự chuyển sang gọi `events_drain` — vẫn KHÔNG poll
 * từng tài nguyên, chỉ có 1 kênh duy nhất và chỉ gọi khi cần.
 *
 * Các view đăng ký bằng `onBackendEvent(pattern, handler)`:
 *   onBackendEvent('task.updated', fn)       // tên chính xác
 *   onBackendEvent('server.*', fn)           // wildcard
 *   onBackendEvent(/^(instance|instances)\./, fn)
 */
import { call } from './bridge.js';

/** Nhịp kiểm tra sức khoẻ kênh đẩy + drain dự phòng. */
const FALLBACK_INTERVAL_MS = 2500;
/** Kênh đẩy im lặng lâu hơn mức này thì coi như hỏng -> dùng drain. */
const PUSH_SILENCE_MS = 6000;

const subscriptions = new Set();
const modeListeners = new Set();

let started = false;
let timer = null;
let lastPushAt = 0;
let draining = false;
let mode = 'idle'; // idle | push | fallback

/* ------------------------------------------------------------------ */
/* Đăng ký                                                             */
/* ------------------------------------------------------------------ */

function toMatcher(pattern) {
  if (pattern === null || pattern === undefined || pattern === '*' || pattern === '') {
    return () => true;
  }
  if (pattern instanceof RegExp) return (name) => pattern.test(name);
  const text = String(pattern);
  if (text.includes('*')) {
    const re = new RegExp(`^${text.split('*').map(escapeRe).join('.*')}$`);
    return (name) => re.test(name);
  }
  return (name) => name === text;
}

function escapeRe(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Lắng nghe event backend. Trả về hàm huỷ đăng ký.
 * @param {string|RegExp|null} pattern
 * @param {(entry:{event:string,payload:object,taskId?:string})=>void} handler
 */
export function onBackendEvent(pattern, handler) {
  const sub = { match: toMatcher(pattern), handler };
  subscriptions.add(sub);
  return () => subscriptions.delete(sub);
}

export function eventMode() {
  return mode;
}

/**
 * Đăng ký nhiều pattern cùng lúc. Hữu ích khi một view nghe cả
 * state-change lẫn data event:
 *   onBackendEvents(['server.*', 'servers.changed'], (e) => ...)
 * Trả về hàm huỷ gỡ toàn bộ.
 */
export function onBackendEvents(patterns, handler) {
  const offs = (Array.isArray(patterns) ? patterns : [patterns])
    .map((p) => onBackendEvent(p, handler));
  return () => offs.forEach((off) => off());
}

export function onEventModeChange(fn) {
  modeListeners.add(fn);
  return () => modeListeners.delete(fn);
}

function setMode(next) {
  if (next === mode) return;
  mode = next;
  for (const fn of modeListeners) {
    try { fn(mode); } catch (e) { console.error('[events] mode listener', e); }
  }
}

/* ------------------------------------------------------------------ */
/* Phát event                                                          */
/* ------------------------------------------------------------------ */

function handle(entry) {
  if (!entry || typeof entry !== 'object') return;
  const name = entry.event || entry.name;
  if (!name) return;

  for (const sub of [...subscriptions]) {
    if (!sub.match(name)) continue;
    try {
      sub.handler(entry);
    } catch (e) {
      console.error(`[events] handler lỗi cho ${name}`, e);
    }
  }

  // Tương thích ngược: component cũ dùng bridge.onEvent()
  window.dispatchEvent(new CustomEvent('pywebview-event', { detail: entry }));
}

export function emitLocalEvent(name, payload = {}) {
  handle({ event: name, payload, timestamp: Date.now() / 1000, local: true });
}

/* ------------------------------------------------------------------ */
/* Kênh đẩy + fallback                                                 */
/* ------------------------------------------------------------------ */

export function startEvents() {
  if (started) return;
  started = true;

  // Kênh đẩy: EventBridge gọi hàm này với 1 batch.
  window.__antaresEvent = (batch) => {
    const list = Array.isArray(batch) ? batch : [batch];
    if (list.length) lastPushAt = Date.now();
    setMode('push');
    list.forEach(handle);
  };

  // Dự phòng: chỉ drain khi kênh đẩy im lặng (không phải poll định kỳ tài nguyên).
  timer = setInterval(async () => {
    if (draining) return;
    if (Date.now() - lastPushAt < PUSH_SILENCE_MS && mode === 'push') return;
    draining = true;
    try {
      const res = await call('events_drain');
      if (!res?.ok) return;
      if (res.data?.pushing) lastPushAt = Date.now();
      setMode(res.data?.pushing ? 'push' : 'fallback');
      (res.data?.events || []).forEach(handle);
    } finally {
      draining = false;
    }
  }, FALLBACK_INTERVAL_MS);
}

/** Drain ngay (dùng khi cửa sổ lấy lại focus — bù event có thể bị lỡ). */
export async function drainNow() {
  if (draining) return 0;
  draining = true;
  try {
    const res = await call('events_drain');
    if (!res?.ok) return 0;
    if (res.data?.pushing) lastPushAt = Date.now();
    setMode(res.data?.pushing ? 'push' : 'fallback');
    const list = res.data?.events || [];
    list.forEach(handle);
    return list.length;
  } finally {
    draining = false;
  }
}

export function stopEvents() {
  if (timer) clearInterval(timer);
  timer = null;
  started = false;
  subscriptions.clear();
}
