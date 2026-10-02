/**
 * Error mapper — biến structured error {code, message, recoverable, action}
 * thành UI: message đã dịch theo ngôn ngữ hiện tại + action đề xuất (mục 215, 365).
 *
 * Ưu tiên: dịch theo code > message gốc từ backend > fallback.
 */
import { t } from '../i18n/i18n.js';
import { toastError } from './toast.js';
import { navigate } from './router.js';
import { icon } from './icons.js';

/** Code → action mặc định khi backend không gợi ý (mục 365 one-click recovery). */
const DEFAULT_ACTIONS = {
  JAVA_NOT_FOUND: 'OPEN_JAVA_SETTINGS',
  JAVA_VERSION_TOO_OLD: 'OPEN_JAVA_SETTINGS',
  AUTH_FAILED: 'OPEN_ACCOUNTS',
  INSTANCE_NOT_FOUND: null,
  MINECRAFT_VERSION_NOT_FOUND: 'INSTALL_VERSION',
  NETWORK_OFFLINE: 'RETRY',
  NETWORK_TIMEOUT: 'RETRY',
  DOWNLOAD_CHECKSUM_MISMATCH: 'RETRY',
  INSTANCE_LOCKED: null,
  SERVER_PORT_BUSY: 'CHANGE_PORT',
};

/** Action → route khi bấm (mapping sang tab tương ứng). */
const ACTION_ROUTES = {
  OPEN_ACCOUNTS: 'accounts',
  OPEN_JAVA_SETTINGS: 'settings',
  INSTALL_VERSION: 'instances',
  RETRY: null, // do caller xử lý retry
  CHANGE_PORT: 'servers',
  VIEW_LOGS: 'logs',
};

/**
 * Xử lý một structured error: hiện toast đã dịch + nút action.
 * @param {object} err  {code, message, recoverable?, action?} từ backend
 * @param {object} opts { goTo: bool — nhảy tab khi bấm action }
 */
export function handleError(err, opts = {}) {
  if (!err) return;
  const code = err.code || 'INTERNAL_ERROR';
  const message = t(`errors.${code}`, {}) !== `errors.${code}`
    ? t(`errors.${code}`)
    : (err.message || t('common.error'));

  const actionKey = err.action || DEFAULT_ACTIONS[code];

  if (actionKey && opts.goTo !== false) {
    toastWithAction(message, actionKey);
  } else {
    toastError(message);
  }
}

function toastWithAction(message, actionKey) {
  const actionLabel = t(`errors.action.${actionKey}`);
  // Toast hiện có: thêm vào DOM với nút action
  const container = document.getElementById('toasts');
  if (!container) { toastError(message); return; }

  const el = document.createElement('div');
  el.className = 'toast error';
  el.innerHTML = `
    <span class="toast-icon">${icon('alert', 16)}</span>
    <span class="toast-body">
      <span>${escapeHtml(message)}</span>
      ${actionLabel !== `errors.action.${actionKey}`
        ? `<button class="btn sm" data-act>${escapeHtml(actionLabel)}</button>`
        : ''}
    </span>`;
  container.appendChild(el);

  const dismiss = () => {
    if (!el.isConnected) return;
    el.classList.add('leaving');
    setTimeout(() => el.remove(), 160);
  };

  el.querySelector('[data-act]')?.addEventListener('click', () => {
    const route = ACTION_ROUTES[actionKey];
    if (route) navigate(route);
    dismiss();
  });

  setTimeout(dismiss, 10000); // error persistent lâu hơn (mục 474)
}

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

/** Wrap một bridge call: lỗi → handleError tự động, thành công → trả data. */
export async function guardedCall(promise, opts = {}) {
  try {
    const res = await promise;
    if (!res.ok) {
      handleError(res.error, opts);
      return null;
    }
    return res.data;
  } catch (e) {
    handleError({ code: 'BRIDGE_ERROR', message: String(e) }, opts);
    return null;
  }
}
