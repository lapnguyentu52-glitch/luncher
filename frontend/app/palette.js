/**
 * Command palette — Ctrl+K để nhảy nhanh giữa các trang & hành động.
 * Không dựng DOM sẵn trong index.html: palette tự tạo và tái sử dụng.
 */
import { routeList, navigate, currentRoute } from './router.js';
import { t } from '../i18n/i18n.js';
import { icon } from './icons.js';

let root = null;
let inputEl = null;
let listEl = null;
let extraActions = [];
let results = [];
let cursor = 0;

/** Đăng ký hành động ngoài (shell truyền vào) — tránh import vòng. */
export function setPaletteActions(actions) {
  extraActions = actions;
}

function buildItems() {
  const nav = routeList().map((r) => ({
    kind: 'nav',
    id: `nav:${r.id}`,
    label: t(r.titleKey),
    hint: t(r.group),
    icon: r.icon,
    kbd: r.slot ? `Ctrl ${r.slot}` : '',
    run: () => navigate(r.id),
  }));

  const actions = extraActions.map((a) => ({
    kind: 'action',
    id: `act:${a.id}`,
    label: a.label,
    hint: t('cmdk.group.actions'),
    icon: a.icon || 'sparkle',
    kbd: a.kbd || '',
    run: a.run,
  }));

  return [...nav, ...actions];
}

function score(item, q) {
  if (!q) return 1;
  const label = item.label.toLowerCase();
  const hay = `${label} ${(item.hint || '').toLowerCase()} ${item.id}`;
  if (label === q) return 100;
  if (label.startsWith(q)) return 80;
  if (label.includes(q)) return 60;
  // subsequence match
  let i = 0;
  for (const ch of hay) {
    if (ch === q[i]) i += 1;
    if (i === q.length) return 30;
  }
  return 0;
}

function render() {
  if (!listEl) return;
  const q = (inputEl.value || '').trim().toLowerCase();
  results = buildItems()
    .map((it) => ({ it, s: score(it, q) }))
    .filter((r) => r.s > 0)
    .sort((a, b) => b.s - a.s)
    .map((r) => r.it)
    .slice(0, 40);

  if (cursor >= results.length) cursor = Math.max(0, results.length - 1);

  if (!results.length) {
    listEl.innerHTML = `<div class="cmdk-empty">${t('cmdk.empty')}</div>`;
    return;
  }

  const active = currentRoute();
  let html = '';
  let lastGroup = null;
  results.forEach((it, idx) => {
    const group = it.kind === 'nav' ? t('cmdk.group.nav') : t('cmdk.group.actions');
    if (group !== lastGroup) {
      html += `<div class="cmdk-group">${group}</div>`;
      lastGroup = group;
    }
    const isCurrent = it.id === `nav:${active}`;
    html += `
      <div class="cmdk-item${idx === cursor ? ' sel' : ''}" data-idx="${idx}" role="option" aria-selected="${idx === cursor}">
        <span class="cmdk-icon">${icon(it.icon, 17)}</span>
        <span class="cmdk-label">${escapeHtml(it.label)}</span>
        ${isCurrent ? `<span class="badge accent">${escapeHtml(t('cmdk.current'))}</span>` : ''}
        ${it.kbd ? `<kbd>${escapeHtml(it.kbd)}</kbd>` : ''}
      </div>`;
  });
  listEl.innerHTML = html;

  listEl.querySelectorAll('[data-idx]').forEach((node) => {
    node.addEventListener('mouseenter', () => {
      cursor = Number(node.dataset.idx);
      highlight();
    });
    node.addEventListener('click', () => runIndex(Number(node.dataset.idx)));
  });
}

function highlight() {
  listEl?.querySelectorAll('[data-idx]').forEach((node) => {
    const on = Number(node.dataset.idx) === cursor;
    node.classList.toggle('sel', on);
    if (on) node.scrollIntoView({ block: 'nearest' });
  });
}

function runIndex(idx) {
  const item = results[idx];
  if (!item) return;
  closePalette();
  // Để palette đóng xong rồi mới điều hướng (tránh giật focus).
  requestAnimationFrame(() => item.run());
}

function ensureDom() {
  if (root) return;
  root = document.createElement('div');
  root.className = 'cmdk';
  root.hidden = true;
  root.innerHTML = `
    <div class="cmdk-scrim" data-cmdk-close></div>
    <div class="cmdk-panel" role="dialog" aria-modal="true" aria-label="${escapeAttr(t('cmdk.title'))}">
      <div class="cmdk-input">
        ${icon('search', 17)}
        <input type="text" id="cmdk-q" autocomplete="off" spellcheck="false"
               placeholder="${escapeAttr(t('cmdk.placeholder'))}" aria-label="${escapeAttr(t('cmdk.title'))}" />
        <kbd>Esc</kbd>
      </div>
      <div class="cmdk-list" id="cmdk-list" role="listbox"></div>
      <div class="cmdk-foot">
        <span class="cmdk-key"><kbd>↑</kbd><kbd>↓</kbd>${escapeHtml(t('cmdk.key.move'))}</span>
        <span class="cmdk-key"><kbd>↵</kbd>${escapeHtml(t('cmdk.key.select'))}</span>
        <span class="cmdk-key"><kbd>Esc</kbd>${escapeHtml(t('cmdk.key.close'))}</span>
      </div>
    </div>`;
  document.body.appendChild(root);

  inputEl = root.querySelector('#cmdk-q');
  listEl = root.querySelector('#cmdk-list');

  inputEl.addEventListener('input', () => { cursor = 0; render(); });
  inputEl.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); cursor = Math.min(cursor + 1, results.length - 1); highlight(); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); cursor = Math.max(cursor - 1, 0); highlight(); }
    else if (e.key === 'Enter') { e.preventDefault(); runIndex(cursor); }
    else if (e.key === 'Escape') { e.preventDefault(); closePalette(); }
  });
  root.querySelectorAll('[data-cmdk-close]').forEach((n) =>
    n.addEventListener('click', () => closePalette()));
}

export function isPaletteOpen() {
  return Boolean(root && !root.hidden);
}

export function openPalette() {
  ensureDom();
  root.querySelector('.cmdk-panel').setAttribute('aria-label', t('cmdk.title'));
  inputEl.placeholder = t('cmdk.placeholder');
  inputEl.value = '';
  cursor = 0;
  root.hidden = false;
  render();
  requestAnimationFrame(() => inputEl.focus());
}

export function closePalette() {
  if (!root || root.hidden) return;
  root.hidden = true;
  inputEl.value = '';
}

export function togglePalette() {
  if (isPaletteOpen()) closePalette();
  else openPalette();
}

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

function escapeAttr(s) {
  return escapeHtml(s);
}
