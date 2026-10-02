/**
 * Router — hash routing (#/dashboard) + route registry (spec mục 185-187).
 *
 * - Một nguồn sự thật cho nav: sidebar, breadcrumb, command palette, hotkey.
 * - Mỗi tab mount trong `<div class="view">` để có animation + scope DOM riêng.
 * - View lỗi không làm sập shell: có error boundary + nút thử lại.
 */
import { DashboardView } from '../tabs/dashboard/dashboard.js';
import { PlayView } from '../tabs/play/play.js';
import { AccountsView } from '../tabs/accounts/accounts.js';
import { InstancesView } from '../tabs/instances/instances.js';
import { ModsView } from '../tabs/mods/mods.js';
import { ServersView } from '../tabs/servers/servers.js';
import { LogsView } from '../tabs/logs/logs.js';
import { DownloadsView } from '../tabs/downloads/downloads.js';
import { DiagnosticsView } from '../tabs/diagnostics/diagnostics.js';
import { SettingsView } from '../tabs/settings/settings.js';
import { SecurityView } from '../tabs/security/security.js';
import { PerformanceView } from '../tabs/performance/performance.js';
import { GameOptimizationView } from '../tabs/game-optimization/game-optimization.js';
import { SystemOptimizationView } from '../tabs/system-optimization/system-optimization.js';
import { ResourceStudioView } from '../tabs/resource-studio/resource-studio.js';
import { VisualStudioView } from '../tabs/visual-studio/visual-studio.js';
import { BackupsView } from '../tabs/backups/backups.js';
import { RepairView } from '../tabs/repair/repair.js';
import { PluginsView } from '../tabs/plugins/plugins.js';
import { ProfilesView } from '../tabs/profiles/profiles.js';
import { SkinsView } from '../tabs/skins/skins.js';
import { t } from '../i18n/i18n.js';
import { icon } from './icons.js';

/** Thứ tự group trong sidebar (spec mục 186; 3.0 mục 50 — GAME group có Performance). */
export const NAV_GROUPS = [
  { key: 'nav.group.home', items: ['dashboard', 'play'] },
  { key: 'nav.group.manage', items: ['instances', 'profiles', 'skins', 'mods', 'resourceStudio', 'visualStudio', 'gameOptimization', 'accounts', 'servers', 'security'] },
  { key: 'nav.group.system', items: ['downloads', 'performance', 'systemOptimization', 'backups', 'repair', 'logs', 'diagnostics'] },
  { key: 'nav.group.app', items: ['settings', 'plugins'] },
];

/**
 * Route registry. `slot` = phím tắt Ctrl+<slot>.
 * view = null nghĩa là tab chưa có implementation (hiện ComingSoon).
 */
export const ROUTES = {
  dashboard: { titleKey: 'nav.dashboard', icon: 'dashboard', group: 'nav.group.home', slot: 1, view: DashboardView },
  play: { titleKey: 'nav.play', icon: 'play', group: 'nav.group.home', slot: 2, view: PlayView },
  instances: { titleKey: 'nav.instances', icon: 'instances', group: 'nav.group.manage', slot: 3, view: InstancesView },
  profiles: { titleKey: 'nav.profiles', icon: 'star', group: 'nav.group.manage', slot: null, view: ProfilesView },
  skins: { titleKey: 'nav.skins', icon: 'user', group: 'nav.group.manage', slot: null, view: SkinsView },
  accounts: { titleKey: 'nav.accounts', icon: 'accounts', group: 'nav.group.manage', slot: 4, view: AccountsView },
  mods: { titleKey: 'nav.mods', icon: 'mods', group: 'nav.group.manage', slot: 5, view: ModsView },
  servers: { titleKey: 'nav.servers', icon: 'servers', group: 'nav.group.manage', slot: 6, view: ServersView },
  security: { titleKey: 'nav.security', icon: 'security', group: 'nav.group.manage', slot: 7, view: SecurityView },
  downloads: { titleKey: 'nav.downloads', icon: 'downloads', group: 'nav.group.system', slot: 8, view: DownloadsView },
  logs: { titleKey: 'nav.logs', icon: 'logs', group: 'nav.group.system', slot: 9, view: LogsView },
  diagnostics: { titleKey: 'nav.diagnostics', icon: 'diagnostics', group: 'nav.group.system', slot: null, view: DiagnosticsView },
  settings: { titleKey: 'nav.settings', icon: 'settings', group: 'nav.group.app', slot: null, view: SettingsView },
  performance: { titleKey: 'nav.performance', icon: 'diagnostics', group: 'nav.group.system', slot: null, view: PerformanceView },
  gameOptimization: { titleKey: 'nav.gameOptimization', icon: 'rocket', group: 'nav.group.manage', slot: null, view: GameOptimizationView },
  systemOptimization: { titleKey: 'nav.systemOptimization', icon: 'power', group: 'nav.group.system', slot: null, view: SystemOptimizationView },
  resourceStudio: { titleKey: 'nav.resourceStudio', icon: 'layers', group: 'nav.group.manage', slot: null, view: ResourceStudioView },
  visualStudio: { titleKey: 'nav.visualStudio', icon: 'sparkle', group: 'nav.group.manage', slot: null, view: VisualStudioView },
  backups: { titleKey: 'nav.backups', icon: 'database', group: 'nav.group.system', slot: null, view: BackupsView },
  repair: { titleKey: 'nav.repair', icon: 'shieldCheck', group: 'nav.group.system', slot: null, view: RepairView },
  plugins: { titleKey: 'nav.plugins', icon: 'mods', group: 'nav.group.app', slot: null, view: PluginsView },
};

export const DEFAULT_ROUTE = 'dashboard';

let currentName = null;
let currentView = null;
const listeners = new Set();

/* ------------------------------------------------------------------ */
/* Public helpers                                                      */
/* ------------------------------------------------------------------ */

export function currentRoute() {
  return currentName;
}

export function routeMeta(name) {
  return ROUTES[name] || null;
}

export function isValidRoute(name) {
  return Object.prototype.hasOwnProperty.call(ROUTES, name);
}

/** Danh sách route phẳng theo đúng thứ tự sidebar. */
export function routeList() {
  return NAV_GROUPS.flatMap((g) => g.items.map((id) => ({ id, ...ROUTES[id] })));
}

/** Đăng ký nhận thông báo đổi route. Trả về hàm huỷ. */
export function onRouteChange(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Tên group hiển thị (đã dịch) của một route. */
export function groupLabelOf(name) {
  const meta = ROUTES[name];
  return meta ? t(meta.group) : '';
}

export function titleOf(name) {
  const meta = ROUTES[name];
  return meta ? t(meta.titleKey) : '';
}

/* ------------------------------------------------------------------ */
/* Navigation                                                          */
/* ------------------------------------------------------------------ */

/**
 * Chuyển tab. Cập nhật hash để back/forward hoạt động.
 * @param {string} name
 * @param {{replace?: boolean, silentHash?: boolean}} [opts]
 */
export function navigate(name, opts = {}) {
  const target = isValidRoute(name) ? name : DEFAULT_ROUTE;
  if (!opts.silentHash) {
    const nextHash = `#/${target}`;
    if (window.location.hash !== nextHash) {
      if (opts.replace) {
        window.history.replaceState(null, '', nextHash);
      } else {
        window.location.hash = nextHash;
        return; // hashchange sẽ gọi render()
      }
    }
  }
  render(target);
}

function readHash() {
  const raw = (window.location.hash || '').replace(/^#\/?/, '').trim();
  const name = raw.split(/[/?]/)[0];
  return isValidRoute(name) ? name : DEFAULT_ROUTE;
}

async function render(name) {
  const content = document.getElementById('content');
  if (!content) return;

  // Rời view cũ (dọn timer/poll trong unmount — spec 28)
  if (currentView?.unmount) {
    try { currentView.unmount(); } catch (e) { console.error('[router] unmount', e); }
  }
  currentView = null;
  currentName = name;

  const host = document.createElement('div');
  host.className = 'view';
  host.dataset.route = name;
  content.replaceChildren(host);
  content.scrollTop = 0;

  const meta = ROUTES[name];
  document.title = `${t(meta.titleKey)} · ${t('app.name')}`;

  if (!meta.view) {
    host.innerHTML = comingSoon(meta);
  } else {
    try {
      currentView = new meta.view(host);
      await currentView.mount?.();
    } catch (err) {
      console.error('[router] mount failed', err);
      currentView = null;
      host.innerHTML = errorState(name);
      bindRetry(host, name);
    }
  }

  notify(name, meta);
}

function notify(name, meta) {
  for (const fn of listeners) {
    try { fn(name, meta); } catch (e) { console.error('[router] listener', e); }
  }
}

/* ------------------------------------------------------------------ */
/* Fallback views                                                      */
/* ------------------------------------------------------------------ */

function comingSoon(meta) {
  const name = meta.titleKey.split('.').pop();
  const key = `${name}.comingSoon`;
  const translated = t(key);
  const body = translated === key ? t('common.comingSoon') : translated;
  return `
    <div class="view-title">${t(meta.titleKey)}</div>
    <div class="view-sub">${t('common.comingSoonSub')}</div>
    <div class="empty">
      <div class="big">${icon(meta.icon, 26)}</div>
      <div class="empty-title">${t(meta.titleKey)}</div>
      <div>${body}</div>
    </div>`;
}

function errorState(name) {
  return `
    <div class="view-title">${t(ROUTES[name].titleKey)}</div>
    <div class="empty">
      <div class="big">${icon('alert', 26)}</div>
      <div class="empty-title">${t('common.viewError')}</div>
      <button class="btn primary" data-retry>${icon('refresh', 15, 'btn-icon')}${t('common.retry')}</button>
    </div>`;
}

function bindRetry(host, name) {
  host.querySelector('[data-retry]')?.addEventListener('click', () => render(name));
}

/* ------------------------------------------------------------------ */
/* Bootstrap wiring                                                    */
/* ------------------------------------------------------------------ */

// Guard môi trường: cho phép import headless (Node/UI smoke test mục 46)
// mà không văng ReferenceError; trong webview DOM vẫn wire đầy đủ.
const HAS_DOM = typeof window !== 'undefined' && typeof document !== 'undefined';

if (HAS_DOM) {
  window.addEventListener('hashchange', () => render(readHash()));

  // Tương thích ngược: component cũ dispatch `window.dispatchEvent(new CustomEvent('nav', ...))`
  window.addEventListener('nav', (e) => navigate(e.detail));
}

// Deep-link khi mở app lần đầu.
export function startRouter() {
  const initial = readHash();
  if (window.location.hash !== `#/${initial}`) {
    window.history.replaceState(null, '', `#/${initial}`);
  }
  return render(initial);
}
