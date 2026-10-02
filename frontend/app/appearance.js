/**
 * Appearance — theme / accent / animations / sidebar (spec mục 154, 241).
 *
 * Single source of truth cho các thuộc tính thị giác; persist vào settings.ui
 * và phát thông báo cho shell + Settings tab.
 */
import { call } from './bridge.js';

export const THEMES = ['dark', 'light'];
export const ACCENTS = ['red', 'gold', 'blue', 'violet', 'teal', 'magenta'];
export const ANIMATIONS = ['full', 'reduced', 'off'];
export const FONT_SCALES = ['0.85', '1', '1.1', '1.25'];
export const DENSITIES = ['comfortable', 'compact'];

const state = {
  theme: 'dark',
  accent: 'red',
  animations: 'full',
  sidebarCollapsed: false,
  fontScale: '1',
  density: 'comfortable',
};

const listeners = new Set();

export function getAppearance() {
  return { ...state };
}

export function onAppearanceChange(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function notify() {
  for (const fn of listeners) {
    try { fn(getAppearance()); } catch (e) { console.error('[appearance] listener', e); }
  }
}

/** Áp token lên <html> — tức thời, không cần reload. */
export function applyAppearance(patch = {}) {
  if (THEMES.includes(patch.theme)) state.theme = patch.theme;
  if (ACCENTS.includes(patch.accent)) state.accent = patch.accent;
  if (ANIMATIONS.includes(patch.animations)) state.animations = patch.animations;
  if (typeof patch.sidebarCollapsed === 'boolean') state.sidebarCollapsed = patch.sidebarCollapsed;
  if (FONT_SCALES.includes(String(patch.fontScale))) state.fontScale = String(patch.fontScale);
  if (DENSITIES.includes(patch.density)) state.density = patch.density;

  const root = document.documentElement;
  root.dataset.theme = state.theme;
  root.dataset.accent = state.accent;
  root.dataset.motion = state.animations;
  root.dataset.density = state.density;
  root.style.setProperty('--font-scale', state.fontScale);
  notify();
  return getAppearance();
}

/** Đọc từ backend rồi áp dụng. An toàn khi backend chưa sẵn sàng. */
export async function loadAppearance() {
  try {
    const res = await call('settings_get');
    if (res?.ok) applyAppearance(res.data?.ui || {});
  } catch { /* dùng default */ }
  return getAppearance();
}

/** Đổi + persist (atomic write ở backend, spec mục 29). */
export async function setAppearance(patch) {
  applyAppearance(patch);
  try {
    await call('settings_update', 'ui', patch);
  } catch { /* giữ thay đổi cho phiên hiện tại */ }
  return getAppearance();
}

/** Bật/tắt nhanh theme. */
export async function toggleTheme() {
  return setAppearance({ theme: state.theme === 'dark' ? 'light' : 'dark' });
}

/** Bật/tắt nhanh sidebar. */
export async function toggleSidebar() {
  return setAppearance({ sidebarCollapsed: !state.sidebarCollapsed });
}
