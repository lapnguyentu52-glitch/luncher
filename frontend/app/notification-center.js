/**
 * Notification Center — drawer + nút chuông + badge (spec 3.0 mục 6, 80).
 *
 * - Nút chuông ở topbar: badge số chưa đọc, click mở drawer bên phải.
 * - Drawer: danh sách realtime từ notification-store, action per-item
 *   (đọc / xoá) + "đọc tất cả" / "xoá tất cả".
 * - Event `notification.created` -> toast ngắn (đường qua store, không poll).
 *   Toast chỉ với severity nổi bật; INFO im lặng để không spam (mục 80).
 * - Policy mục 80: duplicate collapse (store đã lo), max toasts giới hạn
 *   (toast.js dedupe theo message), lỗi sticky lâu hơn.
 */
import { icon } from './icons.js';
import { t } from '../i18n/i18n.js';
import { toast } from './toast.js';
import { onBackendEvent } from './events.js';
import * as store from '../state/notification-store.js';

const SEVERITY_ICON = {
  INFO: 'info',
  SUCCESS: 'check',
  WARNING: 'alert',
  ERROR: 'alert',
  CRITICAL: 'alert',
};
const SEVERITY_TONE = {
  INFO: 'info',
  SUCCESS: 'online',
  WARNING: 'warn',
  ERROR: 'offline',
  CRITICAL: 'offline',
};
/** Severity nào được toast ngắn khi tới realtime (mục 80: không spam). */
const TOAST_SEVERITIES = new Set(['WARNING', 'ERROR', 'CRITICAL']);

let drawerHost = null;
let started = false;
let lastUnread = 0; // giữ badge đúng khi topbar render lại (đổi ngôn ngữ/theme)

/* ------------------------------------------------------------------ */
/* Tiện ích                                                            */
/* ------------------------------------------------------------------ */

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

function titleOf(n) {
  // Backend gửi i18n key (vd "minecraftStarted") — dịch; không có key thì dùng nguyên văn.
  const key = `notif.title.${n.title}`;
  const translated = t(key);
  return translated === key ? (n.title || '') : translated;
}

function timeAgo(ts) {
  const sec = Math.max(0, Math.floor(Date.now() / 1000 - Number(ts || 0)));
  if (sec < 60) return t('notif.time.now');
  if (sec < 3600) return t('notif.time.min', { count: Math.floor(sec / 60) });
  if (sec < 86400) return t('notif.time.hour', { count: Math.floor(sec / 3600) });
  return t('notif.time.day', { count: Math.floor(sec / 86400) });
}

/* ------------------------------------------------------------------ */
/* Nút chuông trên topbar                                              */
/* ------------------------------------------------------------------ */

export function createBellButton() {
  const btn = document.createElement('button');
  btn.className = 'icon-btn has-count';
  btn.id = 'tb-bell';
  btn.title = t('notif.center');
  btn.setAttribute('aria-label', t('notif.center'));
  btn.innerHTML = `${icon('bell', 18)}<span class="task-badge hidden" id="tb-bell-count">0</span>`;
  btn.addEventListener('click', () => toggle());
  // Re-render topbar -> element mới; sync ngay từ trạng thái store hiện tại.
  updateBellBadge(lastUnread);
  return btn;
}

export function updateBellBadge(unread) {
  lastUnread = unread;
  const badge = document.getElementById('tb-bell-count');
  if (!badge) return;
  badge.textContent = unread > 99 ? '99+' : String(unread);
  badge.classList.toggle('hidden', unread === 0);
}

/* ------------------------------------------------------------------ */
/* Drawer                                                              */
/* ------------------------------------------------------------------ */

function ensureDrawer() {
  if (drawerHost) return drawerHost;

  drawerHost = document.createElement('div');
  drawerHost.id = 'notif-drawer';
  drawerHost.className = 'notif-drawer';
  drawerHost.setAttribute('role', 'dialog');
  drawerHost.setAttribute('aria-label', t('notif.center'));
  drawerHost.innerHTML = `
    <div class="notif-scrim" data-notif-close></div>
    <div class="notif-panel card glass">
      <div class="notif-head">
        <div class="notif-title">${icon('bell', 17)}<span>${esc(t('notif.center'))}</span></div>
        <div class="notif-head-actions">
          <button class="btn ghost sm" data-notif-read-all>${icon('check', 14, 'btn-icon')}${esc(t('notif.readAll'))}</button>
          <button class="btn ghost sm" data-notif-clear-all>${icon('trash', 14, 'btn-icon')}${esc(t('notif.clearAll'))}</button>
          <button class="icon-btn plain" data-notif-close aria-label="${esc(t('common.close'))}">${icon('close', 16)}</button>
        </div>
      </div>
      <div class="notif-list" id="notif-list"></div>
    </div>`;

  drawerHost.querySelectorAll('[data-notif-close]').forEach((el) =>
    el.addEventListener('click', () => close()));

  drawerHost.querySelector('[data-notif-read-all]')?.addEventListener('click', () => {
    store.markRead(null);
  });
  drawerHost.querySelector('[data-notif-clear-all]')?.addEventListener('click', () => {
    store.clear(null);
  });

  document.body.appendChild(drawerHost);
  return drawerHost;
}

function renderItems(state) {
  const list = document.getElementById('notif-list');
  if (!list) return;

  if (!state.items.length) {
    list.innerHTML = `
      <div class="empty">
        <div class="big">${icon('bell', 26)}</div>
        <div class="empty-title">${esc(t('notif.empty'))}</div>
        <div>${esc(t('notif.emptySub'))}</div>
      </div>`;
    return;
  }

  list.innerHTML = state.items.map((n) => `
    <div class="notif-item ${n.read ? 'read' : 'unread'} sev-${(n.severity || 'INFO').toLowerCase()}"
         data-notif-id="${esc(n.id)}">
      <span class="notif-icon">${icon(SEVERITY_ICON[n.severity] || 'info', 15)}</span>
      <div class="notif-body">
        <div class="notif-row">
          <b class="notif-item-title">${esc(titleOf(n))}</b>
          <span class="badge ${SEVERITY_TONE[n.severity] || 'info'} notif-sev">${esc(n.severity || 'INFO')}</span>
          <span class="notif-time muted">${esc(timeAgo(n.timestamp))}</span>
        </div>
        ${n.message ? `<div class="notif-msg muted">${esc(n.message)}</div>` : ''}
      </div>
      <div class="notif-item-actions">
        ${!n.read ? `<button class="icon-btn plain" data-notif-read title="${esc(t('notif.markRead'))}"
             aria-label="${esc(t('notif.markRead'))}">${icon('check', 14)}</button>` : ''}
        <button class="icon-btn plain" data-notif-remove title="${esc(t('notif.dismiss'))}"
             aria-label="${esc(t('notif.dismiss'))}">${icon('close', 14)}</button>
      </div>
    </div>`).join('');

  list.querySelectorAll('[data-notif-read]').forEach((b) =>
    b.addEventListener('click', () => store.markRead(b.closest('[data-notif-id]')?.dataset.notifId)));
  list.querySelectorAll('[data-notif-remove]').forEach((b) =>
    b.addEventListener('click', () => store.clear(b.closest('[data-notif-id]')?.dataset.notifId)));
}

/* ------------------------------------------------------------------ */
/* Mở / đóng                                                           */
/* ------------------------------------------------------------------ */

export function open() {
  const host = ensureDrawer();
  host.classList.add('open');
  store.markRead(null); // mở = đã xem toàn bộ (đơn giản, rõ nghĩa)
}

export function close() {
  drawerHost?.classList.remove('open');
}

export function toggle() {
  drawerHost?.classList.contains('open') ? close() : open();
}

/* ------------------------------------------------------------------ */
/* Khởi động 1 lần                                                     */
/* ------------------------------------------------------------------ */

export function startNotificationCenter() {
  if (started) return;
  started = true;

  store.seed();

  // Badge realtime + toast cho severity nổi bật.
  store.onChange((state) => {
    updateBellBadge(state.unread);
    renderItems(state);
    if (isOpen()) store.markRead(null);
  });

  // Toast realtime: chỉ WARNING trở lên (INFO/SUCCESS đi qua các toast cũ
  // của luồng business đã đủ; tránh double-noise — mục 80).
  onBackendEvent('notification.created', (e) => {
    const n = e.payload || {};
    if (!TOAST_SEVERITIES.has(n.severity)) return;
    const msg = `${titleOf(n)}${n.message ? ` — ${n.message}` : ''}`;
    toast(msg, n.severity === 'ERROR' || n.severity === 'CRITICAL' ? 'error' : 'warn',
          n.severity === 'CRITICAL' ? 12000 : 8000);
  });
}
