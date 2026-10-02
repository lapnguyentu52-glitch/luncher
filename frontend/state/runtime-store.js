/**
 * Runtime store — companion session + game telemetry (mục 5 Minecraft runtime).
 *
 * - Sống bằng event `runtime.*` (backend đẩy, đã coalesce latest) — không poll.
 * - FPS history ring để sparkline trong tab Play/Performance.
 */
import { onBackendEvent } from '../app/events.js';
import { call } from '../app/bridge.js';

const MAX_POINTS = 300;

const state = {
  connected: false,
  sessionId: null,
  instanceId: null,
  latest: null,          // {fps, frameMs, low1, ts}
  fpsHistory: [],        // [{ts, fps}]
};

const listeners = new Set();
let wired = false;

function wire() {
  if (wired) return;
  wired = true;

  onBackendEvent('runtime.connected', () => refresh());
  onBackendEvent('runtime.disconnected', () => refresh());

  onBackendEvent('runtime.performance', (e) => {
    const p = e.payload || {};
    state.latest = { fps: p.fps, frameMs: p.frameMs, low1: p.low1, ts: Date.now() };
    if (typeof p.fps === 'number') {
      state.fpsHistory.push([state.latest.ts, p.fps]);
      if (state.fpsHistory.length > MAX_POINTS) state.fpsHistory.splice(0, state.fpsHistory.length - MAX_POINTS);
    }
    emit();
  });
}

function debounce(fn, ms) {
  let timer = null;
  return (...args) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { timer = null; fn(...args); }, ms);
  };
}

const emit = debounce(() => {
  const snap = snapshot();
  for (const fn of [...listeners]) {
    try { fn(snap); } catch (e) { console.error('[runtime-store] listener', e); }
  }
}, 120);

async function refresh() {
  const res = await call('runtime_sessions');
  if (res?.ok) {
    const sessions = res.data.sessions || [];
    const s = sessions[sessions.length - 1] || null;
    state.connected = Boolean(s);
    state.sessionId = s?.sessionId || null;
    state.instanceId = s?.instanceId || null;
    if (!state.connected) { state.latest = null; state.fpsHistory = []; }
    emit();
  }
}

function snapshot() {
  return {
    connected: state.connected,
    sessionId: state.sessionId,
    instanceId: state.instanceId,
    latest: state.latest,
    fpsHistory: [...state.fpsHistory],
  };
}

export function init() {
  wire();
  refresh();
}

export function onChange(fn) {
  wire();
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function getState() {
  return snapshot();
}
