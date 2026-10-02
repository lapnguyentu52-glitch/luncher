/**
 * Frontend bootstrap — dựng shell (sidebar + topbar + statusbar),
 * nối router, hotkey, command palette và appearance (spec mục 185-187).
 *
 * Thứ tự: i18n -> appearance -> chrome -> data -> router -> hide splash.
 */
import { call } from './bridge.js';
import { icon } from './icons.js';
import { initI18n, t, onLanguageChange, setLanguage, getLanguage } from '../i18n/i18n.js';
import {
  NAV_GROUPS, ROUTES, navigate, startRouter, onRouteChange,
  currentRoute, titleOf, groupLabelOf,
} from './router.js';
import {
  loadAppearance, getAppearance, toggleTheme, toggleSidebar, onAppearanceChange,
} from './appearance.js';
import {
  togglePalette, closePalette, isPaletteOpen, setPaletteActions,
} from './palette.js';
import { toast } from './toast.js';
import { applyTooltips } from './tooltip.js';
import {
  startEvents, stopEvents, onBackendEvent, eventMode, onEventModeChange, drainNow,
} from './events.js';
import {
  createBellButton, startNotificationCenter, open as openNotifications, toggle as toggleNotifications,
} from './notification-center.js';

const state = {
  accounts: [],
  instances: [],
  selectedInstance: null,
  selectedAccount: null,
  taskCount: 0,
  /** Số dòng log mới chưa đọc (badge nav Logs, reset khi mở tab). */
  logUnread: 0,
  online: navigator.onLine !== false,
};

/** Bản đồ trạng thái task do event `task.updated` cập nhật — không cần poll. */
const taskStates = new Map();

/* ------------------------------------------------------------------ */
/* helpers                                                             */
/* ------------------------------------------------------------------ */

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const $ = (id) => document.getElementById(id);

/** Gộp nhiều event liên tiếp thành 1 lần refresh (mục 15.2). */
function debounce(fn, ms) {
  let timer = null;
  return (...args) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { timer = null; fn(...args); }, ms);
  };
}

function initials(name) {
  const clean = String(name || '?').trim();
  return clean ? clean[0].toUpperCase() : '?';
}

/* ------------------------------------------------------------------ */
/* Sidebar                                                             */
/* ------------------------------------------------------------------ */

function renderSidebar() {
  $('sidebar')?.setAttribute('aria-label', t('app.name'));
  const nav = $('sidebar-nav');
  if (!nav) return;
  let html = '';
  for (const group of NAV_GROUPS) {
    html += `<div class="group-label">${esc(t(group.key))}</div>`;
    for (const id of group.items) {
      const r = ROUTES[id];
      const label = t(r.titleKey);
      html += `
        <div class="nav-item" data-route="${id}" data-label="${esc(label)}"
             role="button" tabindex="0" aria-label="${esc(label)}">
          <span class="nav-icon">${icon(r.icon, 19)}</span>
          <span class="nav-label">${esc(label)}</span>
          <span class="nav-badge hidden" data-nav-badge="${id}"></span>
          ${r.slot ? `<kbd class="nav-kbd">Ctrl ${r.slot}</kbd>` : ''}
        </div>`;
    }
  }
  nav.innerHTML = html;

  nav.querySelectorAll('.nav-item').forEach((el) => {
    el.addEventListener('click', () => navigate(el.dataset.route));
    el.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); navigate(el.dataset.route); }
    });
  });
  setActiveNav(currentRoute());
  updateLogBadge();
}

/** Badge số dòng log mới trên nav Logs (ẩn khi đang ở tab Logs). */
function updateLogBadge() {
  const badge = document.querySelector('[data-nav-badge="logs"]');
  if (!badge) return;
  const onLogsTab = currentRoute() === 'logs';
  const count = onLogsTab ? 0 : state.logUnread;
  badge.textContent = count > 99 ? '99+' : String(count);
  badge.classList.toggle('hidden', count === 0);
  badge.title = t('nav.newLines', { count });
}

function renderSidebarBrand() {
  const head = $('sidebar-head');
  if (!head) return;
  head.innerHTML = `
    <div class="logo" data-label="${esc(t('app.name'))}">
      <span class="logo-mark">${icon('star', 19)}</span>
      <span class="logo-text">Antares<small>${esc(t('app.version'))}</small></span>
    </div>`;
}

function renderSidebarFooter() {
  const foot = $('sidebar-foot');
  if (!foot) return;
  const acc = state.accounts.find((a) => a.id === state.selectedAccount) || state.accounts[0];
  foot.innerHTML = `
    <div class="sb-status" id="sb-account" role="button" tabindex="0" data-label="${esc(t('nav.accounts'))}">
      <span class="sb-avatar" id="sb-avatar">${esc(initials(acc?.displayName))}</span>
      <span class="sb-meta">
        <b>${esc(acc?.displayName || t('topbar.noAccount'))}</b>
        <span>${esc(acc ? acc.type : t('topbar.account'))}</span>
      </span>
    </div>`;
  const chip = $('sb-account');
  chip?.addEventListener('click', () => navigate('accounts'));
  chip?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); navigate('accounts'); }
  });
}

function setActiveNav(name) {
  document.querySelectorAll('.nav-item').forEach((el) => {
    const on = el.dataset.route === name;
    el.classList.toggle('active', on);
    if (on) el.setAttribute('aria-current', 'page');
    else el.removeAttribute('aria-current');
  });
}

/* ------------------------------------------------------------------ */
/* Topbar + statusbar                                                  */
/* ------------------------------------------------------------------ */

function renderTopbar() {
  const bar = $('topbar');
  if (!bar) return;
  const collapsed = getAppearance().sidebarCollapsed;
  bar.innerHTML = `
    <button class="icon-btn" id="sb-toggle" title="${esc(t('topbar.toggleSidebar'))}"
            aria-label="${esc(t('topbar.toggleSidebar'))}" aria-expanded="${collapsed ? 'false' : 'true'}">
      ${icon('panelLeft', 18)}
    </button>
    <div class="crumbs" id="crumbs"></div>
    <div class="topbar-spacer"></div>
    <button class="search-trigger" id="cmdk-open" title="${esc(t('cmdk.title'))}">
      ${icon('search', 15)}
      <span class="grow">${esc(t('topbar.search'))}</span>
      <kbd>Ctrl K</kbd>
    </button>
    <button class="icon-btn has-count" id="tb-tasks" title="${esc(t('topbar.tasks'))}"
            aria-label="${esc(t('topbar.tasks'))}">
      ${icon('tasks', 18)}
      <span class="task-badge${state.taskCount ? '' : ' hidden'}" id="tb-task-count">${state.taskCount}</span>
    </button>
    <div class="net-pill${state.online ? '' : ' offline'}" id="tb-net">
      <span class="net-dot"></span><span>${esc(state.online ? t('topbar.online') : t('topbar.offline'))}</span>
    </div>
    <button class="avatar-btn" id="tb-account" title="${esc(t('nav.accounts'))}">
      <span class="avatar" id="tb-avatar">${esc(initials(currentAccount()?.displayName))}</span>
      <span class="grow truncate">${esc(currentAccount()?.displayName || t('topbar.noAccount'))}</span>
    </button>`;

  const acc = currentAccount();
  $('tb-avatar').classList.toggle('accent', Boolean(acc));
  $('sb-toggle')?.addEventListener('click', toggleSidebar);
  $('cmdk-open')?.addEventListener('click', togglePalette);
  // Notification Center (spec 3.0 mục 6): chuông nằm trước nút task
  const tasksBtn = $('tb-tasks');
  if (tasksBtn) tasksBtn.before(createBellButton());
  tasksBtn?.addEventListener('click', () => navigate('downloads'));
  $('tb-account')?.addEventListener('click', () => navigate('accounts'));
  updateAccountChips();
  updateCrumbs(currentRoute());
  updateTaskBadge();
  updateNetwork();
  applyTooltips(bar);
}

function currentAccount() {
  return state.accounts.find((a) => a.id === state.selectedAccount) || state.accounts[0] || null;
}

function updateAccountChips() {
  const acc = currentAccount();
  const name = acc?.displayName || t('topbar.noAccount');
  const av = $('tb-avatar');
  if (av) {
    av.textContent = initials(acc?.displayName);
    av.classList.toggle('accent', Boolean(acc));
  }
  const btn = $('tb-account');
  if (btn) {
    const label = btn.querySelector('.grow');
    if (label) label.textContent = name;
  }
  const sbAvatar = $('sb-avatar');
  if (sbAvatar) sbAvatar.textContent = initials(acc?.displayName);
  const sbName = document.querySelector('#sb-account .sb-meta b');
  if (sbName) sbName.textContent = name;
  const sbType = document.querySelector('#sb-account .sb-meta span');
  if (sbType) sbType.textContent = acc ? acc.type : t('topbar.account');
}

function updateCrumbs(name) {
  const crumb = $('crumbs');
  if (!crumb) return;
  crumb.innerHTML = `
    <span class="crumb-group">${esc(groupLabelOf(name))}</span>
    <span class="crumb-sep">/</span>
    <span class="crumb-current">${esc(titleOf(name))}</span>`;
}

function renderStatusbar() {
  let bar = $('statusbar');
  if (!bar) {
    bar = document.createElement('div');
    bar.id = 'statusbar';
    document.body.appendChild(bar);
  }
  bar.innerHTML = `
    <span class="sb-seg">
      <span class="dot" id="sb-dot">●</span>
      <span id="sb-state">${esc(t('common.ready'))}</span>
    </span>
    <span class="sb-seg" id="sb-route"></span>
    <span class="sb-spacer"></span>
    <span class="sb-seg" id="sb-instances"></span>
    <span class="sb-seg" id="sb-tasks"></span>
    <span class="sb-seg" id="sb-events"></span>
    <span class="sb-seg sb-brand">Antares ${esc(t('app.version'))}</span>`;
  updateStatusbar(currentRoute());
  updateEventModeChip();
}

function updateStatusbar(name) {
  const meta = ROUTES[name] || ROUTES.dashboard;
  const route = $('sb-route');
  if (route) route.innerHTML = `${icon(meta.icon, 13)}<span>${esc(titleOf(name) || t('common.ready'))}</span>`;
  const inst = $('sb-instances');
  if (inst) {
    inst.innerHTML = `${icon('instances', 13)}<span>${state.instances.length} ${esc(t('statusbar.instances'))}</span>`;
  }
  updateTaskBadge();
  updateNetwork();
}

function updateTaskBadge() {
  const badge = $('tb-task-count');
  if (badge) {
    badge.textContent = String(state.taskCount);
    badge.classList.toggle('hidden', state.taskCount === 0);
  }
  const seg = $('sb-tasks');
  if (seg) {
    seg.classList.toggle('hidden', state.taskCount === 0);
    seg.innerHTML = `${icon('tasks', 13)}<span>${state.taskCount} ${esc(t('statusbar.tasks'))}</span>`;
  }
}

function updateNetwork() {
  const pill = $('tb-net');
  if (pill) {
    pill.classList.toggle('offline', !state.online);
    const label = pill.querySelector('span:not(.net-dot)');
    if (label) label.textContent = state.online ? t('topbar.online') : t('topbar.offline');
  }
  const dot = $('sb-dot');
  if (dot) dot.classList.toggle('offline', !state.online);
}

/* ------------------------------------------------------------------ */
/* Data                                                            */
/* ------------------------------------------------------------------ */

async function refreshState() {
  const res = await call('app_get_state');
  if (!res?.ok) return;
  const data = res.data || {};
  state.accounts = data.accountsSummary || [];
  state.instances = data.instancesSummary || [];
  state.selectedInstance = data.selectedInstance || null;
  state.selectedAccount = data.selectedAccount || state.selectedAccount || null;
  updateAccountChips();
  updateStatusbar(currentRoute());
}

/** Nạp 1 lần lúc mở app (không phải poll) để badge đúng ngay từ đầu. */
async function seedTasks() {
  const res = await call('tasks_list');
  if (!res?.ok) return;
  for (const task of res.data?.tasks || []) taskStates.set(task.id, task.state);
  recountTasks();
}

function applyTaskEvent(task) {
  if (!task?.id) return;
  taskStates.set(task.id, task.state);
  if (taskStates.size > 300) pruneTaskStates();
  recountTasks();
}

function pruneTaskStates() {
  for (const [id, st] of taskStates) {
    if (st !== 'running' && st !== 'pending') taskStates.delete(id);
  }
}

function recountTasks() {
  let active = 0;
  for (const st of taskStates.values()) {
    if (st === 'running' || st === 'pending') active += 1;
  }
  state.taskCount = active;
  updateTaskBadge();
}

/** Chip trạng thái kênh event trên statusbar (trực tiếp / dự phòng). */
function updateEventModeChip() {
  const el = $('sb-events');
  if (!el) return;
  const live = eventMode() === 'push';
  const key = live ? 'statusbar.live' : 'statusbar.fallback';
  el.innerHTML = `${icon('wifi', 13)}<span>${esc(t(key))}</span>`;
  el.title = t(live ? 'statusbar.liveHint' : 'statusbar.fallbackHint');
  el.classList.toggle('dim', !live);
}

/**
 * Nối event backend -> UI. Thay cho các setInterval poll trước đây:
 * task/progress, log, server, và mọi thay đổi instance/account/settings.
 */
function setupEventWiring() {
  startEvents();

  onBackendEvent('task.updated', (e) => applyTaskEvent(e.payload));

  // Dòng log mới -> badge trên nav Logs khi người dùng đang ở tab khác
  // (batch chỉ mang số lượng nên việc đếm không phụ thuộc nội dung dòng).
  onBackendEvent('log.lines', (e) => {
    if (currentRoute() === 'logs') return;
    const n = Number(e.payload?.count || e.payload?.lines?.length || 0);
    if (n > 0) {
      state.logUnread = Math.min(9999, state.logUnread + n);
      updateLogBadge();
    }
  });

  const refreshSoon = debounce(() => { refreshState(); }, 250);
  onBackendEvent(/^(instance|instances|accounts|auth|selection|settings)\./, refreshSoon);
  onBackendEvent('app.ready', () => { refreshState(); seedTasks(); });

  onEventModeChange(updateEventModeChip);

  // Lấy lại focus = thời điểm dễ lỡ event nhất (webview bị treo/tạm ẩn).
  window.addEventListener('focus', () => { drainNow(); refreshState(); });

  // Chạy nền lần đầu để chip hiển thị đúng mode ngay cả khi backend im lặng.
  drainNow().then(updateEventModeChip);
}

/* ------------------------------------------------------------------ */
/* Appearance / language / hotkeys                                     */
/* ------------------------------------------------------------------ */

function applySidebarState() {
  const app = $('app');
  app?.classList.toggle('sidebar-collapsed', getAppearance().sidebarCollapsed);
  $('sb-toggle')?.setAttribute('aria-expanded', getAppearance().sidebarCollapsed ? 'false' : 'true');
}

function setupPalette() {
  setPaletteActions([
    {
      id: 'toggle-sidebar',
      label: t('action.toggleSidebar'),
      icon: 'panelLeft',
      kbd: 'Ctrl B',
      run: () => toggleSidebar(),
    },
    {
      id: 'toggle-theme',
      label: t('action.toggleTheme'),
      icon: 'sparkle',
      run: () => toggleTheme(),
    },
    {
      id: 'toggle-language',
      label: t('action.toggleLanguage'),
      icon: 'globe',
      run: () => setLanguage(getLanguage() === 'vi' ? 'en' : 'vi'),
    },
    {
      id: 'open-notifications',
      label: t('action.openNotifications'),
      icon: 'bell',
      kbd: 'Ctrl Shift N',
      run: () => openNotifications(),
    },
  ]);
}

function setupHotkeys() {
  window.addEventListener('keydown', (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) {
      if (e.key === 'Escape' && isPaletteOpen()) closePalette();
      return;
    }
    const key = e.key.toLowerCase();

    if (key === 'k' || key === 'p') { e.preventDefault(); togglePalette(); return; }
    if (key === 'b') { e.preventDefault(); toggleSidebar(); return; }
    if (key === 'n' && e.shiftKey) { e.preventDefault(); toggleNotifications(); return; }
    if (key === ',') { e.preventDefault(); navigate('settings'); return; }

    // Ctrl + 1..9 -> tab theo `slot`
    const digit = Number(e.key);
    if (Number.isInteger(digit) && digit >= 1 && digit <= 9) {
      const entry = Object.entries(ROUTES).find(([, r]) => r.slot === digit);
      if (entry) { e.preventDefault(); navigate(entry[0]); }
    }
  });
}

function setupConnectivity() {
  const sync = () => {
    state.online = navigator.onLine !== false;
    updateNetwork();
    if (!state.online) toast(t('common.offlineNotice'), 'warn');
  };
  window.addEventListener('online', sync);
  window.addEventListener('offline', sync);
}

function rerenderChrome() {
  const active = currentRoute();
  renderSidebarBrand();
  renderSidebar();
  renderSidebarFooter();
  renderTopbar();
  renderStatusbar();
  setupPalette();
  applySidebarState();
  document.documentElement.lang = getLanguage();
  // Vẽ lại view hiện tại để text theo ngôn ngữ mới (mục 577).
  navigate(active, { silentHash: true });
}

/* ------------------------------------------------------------------ */
/* Init                                                               */
/* ------------------------------------------------------------------ */

/** Topbar nổi bóng nhẹ khi #content cuộn xuống — cảm giác lớp rõ hơn (spec 187). */
function setupScrollShadow() {
  const content = $('content');
  const bar = $('topbar');
  if (!content || !bar) return;
  const onScroll = () => bar.classList.toggle('scrolled', content.scrollTop > 2);
  content.addEventListener('scroll', onScroll, { passive: true });
  onScroll();
}

function hideSplash() {
  const splash = $('splash');
  if (!splash) return;
  splash.hidden = true;
  setTimeout(() => splash.remove(), 500);
}

async function init() {
  await initI18n();
  await loadAppearance();
  document.documentElement.lang = getLanguage();

  renderSidebarBrand();
  renderSidebar();
  renderSidebarFooter();
  renderTopbar();
  renderStatusbar();
  applySidebarState();
  setupPalette();
  setupHotkeys();
  setupConnectivity();
  setupEventWiring();
  startNotificationCenter();

  onAppearanceChange(applySidebarState);
  onRouteChange((name) => {
    setActiveNav(name);
    updateCrumbs(name);
    updateStatusbar(name);
    applyTooltips($('content'));
    // Vào tab Logs = đã đọc hết -> xoá badge dòng mới.
    if (name === 'logs' && state.logUnread) {
      state.logUnread = 0;
      updateLogBadge();
    }
  });
  setupScrollShadow();
  onLanguageChange(rerenderChrome);

  // Splash tối đa ~1.2s: bridge có timeout 8s, không để app trắng khi backend chậm.
  await Promise.race([startRouter(), new Promise((r) => setTimeout(r, 1200))]);
  hideSplash();

  // Trạng thái app (nhẹ, không block first paint — spec 39)
  refreshState().then(() => {
    if (!currentAccount()) toast(t('common.backendHint'), 'info');
  });
  // Một lần duy nhất lúc mở app; sau đó mọi thay đổi đến từ event.
  seedTasks();
}

// Boot chỉ chạy trong DOM thật (webview). Import headless (UI smoke test
// mục 46) chỉ kiểm module — không tự khởi động app.
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => {
    stopEvents();
  });

  init().catch((e) => {
    console.error('[bootstrap] init failed', e);
    hideSplash();
  });
}
