/**
 * Modal / confirm dialog — thay cho `window.confirm()` (UI đồng bộ, có theme).
 *
 *   const ok = await confirmDialog({ title: '...', message: '...' });
 *   const m = openModal({ title, body, actions: [...] });  m.close();
 */
import { t } from '../i18n/i18n.js';
import { icon } from './icons.js';

let openCount = 0;

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

/**
 * Mở một modal tổng quát.
 * @param {object} opts
 * @param {string} opts.title
 * @param {string} [opts.body]       HTML an toàn (đã escape ở caller)
 * @param {string} [opts.icon]       tên icon
 * @param {'accent'|'danger'|''} [opts.tone]
 * @param {boolean} [opts.wide]
 * @param {Array<{label:string, variant?:string, value?:any, icon?:string, autofocus?:boolean, onClick?:Function}>} [opts.actions]
 * @param {boolean} [opts.dismissible]
 * @returns {{ el: HTMLElement, close: Function }}
 */
export function openModal(opts = {}) {
  const {
    title = '',
    body = '',
    icon: iconName = null,
    tone = '',
    wide = false,
    actions = [],
    dismissible = true,
  } = opts;

  const prevFocus = document.activeElement;
  const el = document.createElement('div');
  el.className = 'modal';
  el.innerHTML = `
    <div class="modal-scrim" data-close></div>
    <div class="modal-panel${wide ? ' wide' : ''}" role="dialog" aria-modal="true">
      ${title ? `
        <div class="modal-head">
          ${iconName ? `<span class="modal-icon ${tone}">${icon(iconName, 18)}</span>` : ''}
          <div class="modal-title">${esc(title)}</div>
          ${dismissible ? `<button class="icon-btn plain" data-close aria-label="${esc(t('common.close'))}">${icon('close', 16)}</button>` : ''}
        </div>` : ''}
      ${body ? `<div class="modal-body">${body}</div>` : ''}
      <div class="modal-foot" data-foot></div>
    </div>`; 

  const foot = el.querySelector('[data-foot]');
  let lastValue;

  const close = (value) => {
    if (!el.isConnected) return;
    el.remove();
    openCount = Math.max(0, openCount - 1);
    if (!openCount && document.body.dataset.modalOpen) delete document.body.dataset.modalOpen;
    window.removeEventListener('keydown', onKey, true);
    if (!actions.length) lastValue = value;
    if (prevFocus?.focus) { try { prevFocus.focus(); } catch { /* noop */ } }
  };

  const pick = (v) => { lastValue = v; close(v); };

  for (const a of actions) {
    const btn = document.createElement('button');
    btn.className = `btn${a.variant ? ` ${a.variant}` : ''}`;
    if (a.icon) btn.innerHTML = `${icon(a.icon, 15, 'btn-icon')}<span>${esc(a.label)}</span>`;
    else btn.textContent = a.label;
    btn.addEventListener('click', () => {
      if (a.onClick) a.onClick({ close, el });
      else pick(a.value ?? a.label);
    });
    if (a.autofocus) queueMicrotask(() => btn.focus());
    foot.appendChild(btn);
  }

  const onKey = (e) => {
    if (e.key === 'Escape' && dismissible) { e.stopPropagation(); close(undefined); return; }
    if (e.key !== 'Tab') return;
    // Focus trap đơn giản trong panel.
    const focusables = el.querySelectorAll('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])');
    if (!focusables.length) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  };

  el.querySelectorAll('[data-close]').forEach((n) => {
    n.addEventListener('click', () => { if (dismissible) close(undefined); });
  });

  document.body.appendChild(el);
  document.body.dataset.modalOpen = '1';
  openCount += 1;
  window.addEventListener('keydown', onKey, true);

  return { el, close, get value() { return lastValue; } };
}

/**
 * Hộp thoại xác nhận. Trả về Promise<boolean>.
 * @param {object} opts { title, message, confirmText?, cancelText?, danger? }
 */
export function confirmDialog(opts = {}) {
  const {
    title = t('common.confirm'),
    message = '',
    confirmText = t('common.confirm'),
    cancelText = t('common.cancel'),
    danger = true,
  } = opts;

  return new Promise((resolve) => {
    let done = false;
    const finish = (v) => { if (!done) { done = true; resolve(v); } };
    const m = openModal({
      title,
      icon: danger ? 'alert' : 'info',
      tone: danger ? 'danger' : 'accent',
      body: message ? `<p>${esc(message)}</p>` : '',
      actions: [
        { label: cancelText, variant: 'ghost', value: false, onClick: ({ close }) => { finish(false); close(); } },
        {
          label: confirmText,
          variant: danger ? 'danger' : 'primary',
          value: true,
          autofocus: true,
          onClick: ({ close }) => { finish(true); close(); },
        },
      ],
    });
    // Đóng bằng scrim/ESC -> huỷ.
    const observer = new MutationObserver(() => {
      if (!m.el.isConnected) { observer.disconnect(); finish(false); }
    });
    observer.observe(document.body, { childList: true });
  });
}

/** Thông báo 1 nút OK. */
export function alertDialog({ title, message, icon: iconName = 'info' } = {}) {
  return new Promise((resolve) => {
    const m = openModal({
      title,
      icon: iconName,
      body: message ? `<p>${esc(message)}</p>` : '',
      actions: [{ label: t('common.close'), variant: 'primary', value: true, autofocus: true }],
    });
    const observer = new MutationObserver(() => {
      if (!m.el.isConnected) { observer.disconnect(); resolve(true); }
    });
    observer.observe(document.body, { childList: true });
  });
}
