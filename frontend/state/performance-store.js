/**
 * Performance store — nguồn sự thật phía UI cho Performance Center (mục 5, 26.2).
 *
 * - Nạp snapshot ban đầu đúng 1 lần (`seed()`), sau đó sống bằng event
 *   `performance.telemetry` (backend đẩy qua EventBridge, đã coalesce latest).
 * - Ring buffer phía UI, giới hạn điểm để graph mượt.
 */
import { call } from '../app/bridge.js';
import { onBackendEvent } from '../app/events.js';

const MAX_POINTS = 900;

const history = { cpu: [], ram: [], diskRead: [], diskWrite: [], launcherCpu: [], launcherRss: [] };
const listeners = new Set();

let latest = null;
let gameMode = false;
let seeded = false;
let seeding = null;

let wired = false;

function wire() {
  if (wired) return;
  wired = true;

  onBackendEvent('performance.telemetry', (e) => {
    const sample = e.payload || {};
    if (typeof sample.ts !== 'number') return;
    latest = sample;
    gameMode = Boolean(sample.gameMode);
    push(sample);
    emit();
  });
}

function push(sample) {
  const t = sample.ts * 1000; // ms cho chart
  pushTo(history.cpu, [t, sample.cpu || 0]);
  pushTo(history.ram, [t, sample.ram?.percent || 0]);
  pushTo(history.diskRead, [t, bpsToMb(sample.disk?.readBps)]);
  pushTo(history.diskWrite, [t, bpsToMb(sample.disk?.writeBps)]);
  pushTo(history.launcherCpu, [t, sample.launcher?.cpu || 0]);
  pushTo(history.launcherRss, [t, (sample.launcher?.rss || 0) / (1024 * 1024)]);
}

const bpsToMb = (v) => (typeof v === 'number' ? v / (1024 * 1024) : 0);

function pushTo(arr, point) {
  arr.push(point);
  if (arr.length > MAX_POINTS) arr.splice(0, arr.length - MAX_POINTS);
}

/** Gộp nhiều event liên tiếp -> 1 lần notify (mục 15.2). */
function debounce(fn, ms) {
  let timer = null;
  return (...args) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { timer = null; fn(...args); }, ms);
  };
}

const emit = debounce(() => {
  for (const fn of [...listeners]) {
    try { fn(snapshot()); } catch (e) { console.error('[perf-store] listener', e); }
  }
}, 150);

function snapshot() {
  return {
    latest,
    gameMode,
    history: {
      cpu: [...history.cpu],
      ram: [...history.ram],
      diskRead: [...history.diskRead],
      diskWrite: [...history.diskWrite],
      launcherCpu: [...history.launcherCpu],
      launcherRss: [...history.launcherRss],
    },
  };
}

/** Nạp 1 lần lúc mở app (history backend -> vẽ ngay không phải đợi). */
export async function seed() {
  wire();
  if (seeded) return snapshot();
  if (seeding) return seeding;
  seeding = (async () => {
    const res = await call('performance_snapshot');
    if (res?.ok) {
      const data = res.data || {};
      gameMode = Boolean(data.gameMode);
      for (const s of data.history || []) push(s);
      latest = data.latest || (data.history || []).slice(-1)[0] || null;
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

export function getState() {
  return snapshot();
}
