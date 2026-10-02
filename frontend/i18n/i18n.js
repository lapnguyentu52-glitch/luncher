/**
 * i18n — mặc định tiếng Việt, auto-detect theo ngôn ngữ máy (mục 575-580).
 *
 * Config "ui.language":
 *   "vi" / "en"  -> dùng cứng
 *   "auto"       -> detect từ navigator.language của máy
 * Mặc định khi chưa config: "vi" (theo yêu cầu), auto-detect sẽ nâng lên
 * "en" nếu máy dùng tiếng Anh.
 * User đổi ngôn ngữ -> persist qua settings_update (mục 577).
 */

import { call } from '../app/bridge.js';

const MESSAGES = { vi: null, en: null };
let currentLang = 'vi';
let listeners = [];

/** Detect ngôn ngữ máy từ navigator.language (mục 575). */
export function detectSystemLanguage() {
  const nav = (navigator.language || navigator.userLanguage || 'vi').toLowerCase();
  if (nav.startsWith('vi')) return 'vi';
  if (nav.startsWith('en')) return 'en';
  return 'vi'; // fallback mặc định tiếng Việt
}

async function loadDict(lang) {
  if (MESSAGES[lang]) return MESSAGES[lang];
  try {
    // URL này resolve so với THƯ MỤC CỦA MODULE (/i18n/) — ../i18n/ = /i18n/
    // (UIServer root = frontend/). Dùng đường dẫn tuyệt đối cho rõ ràng.
    const resp = await fetch(`i18n/${lang}.json`);
    MESSAGES[lang] = await resp.json();
  } catch {
    MESSAGES[lang] = {};
  }
  return MESSAGES[lang];
}

/** Khởi tạo i18n: đọc config backend -> auto detect nếu "auto"/thiếu. */
export async function initI18n() {
  let configured = 'vi';
  try {
    const res = await call('settings_get');
    if (res.ok) configured = res.data.ui?.language || 'vi';
  } catch { /* backend chưa sẵn sàng — dùng vi */ }

  currentLang = (configured === 'auto')
    ? detectSystemLanguage()
    : (['vi', 'en'].includes(configured) ? configured : 'vi');

  await loadDict(currentLang);
  // preload dict còn lại để switch instant (mục 580)
  loadDict(currentLang === 'vi' ? 'en' : 'vi');
  return currentLang;
}

/** Đổi ngôn ngữ + persist (mục 577). */
export async function setLanguage(lang) {
  if (!['vi', 'en', 'auto'].includes(lang)) return currentLang;
  // Lưu raw config ("auto" được giữ nguyên để máy khác detect lại)
  try {
    await call('settings_update', 'ui', { language: lang });
  } catch { /* vẫn đổi tạm cho phiên này */ }

  currentLang = (lang === 'auto') ? detectSystemLanguage() : lang;
  await loadDict(currentLang);
  listeners.forEach(fn => fn(currentLang));
  return currentLang;
}

export function getLanguage() {
  return currentLang;
}

export function onLanguageChange(fn) {
  listeners.push(fn);
  return () => { listeners = listeners.filter(f => f !== fn); };
}

/** Dịch theo key, hỗ trợ {placeholder} (mục 578: không nối chuỗi raw). */
export function t(key, params = {}) {
  const dict = MESSAGES[currentLang] || {};
  let text = dict[key] ?? key;
  for (const [k, v] of Object.entries(params)) {
    text = text.replaceAll(`{${k}}`, String(v));
  }
  return text;
}
