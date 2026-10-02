/** Toast notifications — grouped, auto-hide theo loại (mục 34, 474). */
import { icon } from './icons.js';

const ICONS = { info: 'info', success: 'check', error: 'alert', warn: 'alert' };

const container = () => document.getElementById('toasts');

function dismiss(el) {
  if (!el.isConnected) return;
  el.classList.add('leaving');
  setTimeout(() => el.remove(), 160);
}

/**
 * Hiện toast. Trả về hàm để đóng sớm.
 * @param {string} message
 * @param {'info'|'success'|'error'|'warn'} type
 * @param {number} [durationMs]
 */
export function toast(message, type = 'info', durationMs = 4000) {
  const host = container();
  if (!host) return () => {};

  // Dedupe: cùng message + type đang hiện -> chỉ nháy lại
  const existing = [...host.children].find(
    (n) => n.dataset.message === String(message) && n.dataset.type === type);
  if (existing) {
    existing.classList.remove('leaving');
    clearTimeout(Number(existing.dataset.timer || 0));
    const ttl = type === 'error' ? 10000 : durationMs;
    existing.dataset.timer = String(setTimeout(() => dismiss(existing), ttl));
    return () => dismiss(existing);
  }

  const el = document.createElement('div');
  el.className = `toast ${type}`;
  el.dataset.message = String(message);
  el.dataset.type = type;
  el.innerHTML = `<span class="toast-icon">${icon(ICONS[type] || 'info', 16)}</span>
    <span class="toast-body"></span>`;
  el.querySelector('.toast-body').textContent = message;
  host.appendChild(el);

  const ttl = type === 'error' ? 10000 : durationMs;
  el.dataset.timer = String(setTimeout(() => dismiss(el), ttl));
  el.addEventListener('click', () => dismiss(el));

  return () => dismiss(el);
}

export const toastSuccess = (m) => toast(m, 'success');
export const toastError = (m) => toast(m, 'error');
export const toastInfo = (m) => toast(m, 'info');
export const toastWarn = (m) => toast(m, 'warn');
