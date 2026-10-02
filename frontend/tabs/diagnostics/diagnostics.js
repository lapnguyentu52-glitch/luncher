/** Diagnostics tab — system / webview / java / storage (mục 152). */
import { call } from '../../app/bridge.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { icon } from '../../app/icons.js';
import { t, getLanguage } from '../../i18n/i18n.js';
import { getAppearance } from '../../app/appearance.js';

export class DiagnosticsView {
  constructor(el) { this.el = el; }

  async mount() {
    const [java, state] = await Promise.all([call('java_list'), call('app_get_state')]);
    const javas = java?.ok ? (java.data.javas || []) : [];
    const data = state?.ok ? state.data : {};
    const app = data.app || {};
    const look = getAppearance();
    const nav = navigator;

    const rows = [
      ['diagnostics.platform', `${nav.platform || '?'}`],
      ['diagnostics.browser', `${nav.userAgent || '?'}`],
      ['diagnostics.screen', `${screen.width}×${screen.height} @${window.devicePixelRatio || 1}x`],
      ['diagnostics.cores', String(nav.hardwareConcurrency || '?')],
      ['diagnostics.memory', nav.deviceMemory ? `${nav.deviceMemory} GB` : '—'],
      ['diagnostics.language', `${getLanguage()} (${nav.language || '?'})`],
      ['diagnostics.theme', `${look.theme} · ${look.accent} · ${look.animations}`],
      ['diagnostics.network', nav.onLine ? t('topbar.online') : t('topbar.offline')],
    ];

    this._report = [
      `${app.name || 'Antares'} ${app.version || ''}`,
      ...rows.map(([k, v]) => `${t(k)}: ${v}`),
      `${t('diagnostics.java')}: ${javas.map((j) => `Java ${j.major} (${j.path})`).join(', ') || '-'}`,
    ].join('\n');

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('diagnostics.title')}</div>
          <div class="view-sub">${t('diagnostics.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="diag-copy">${icon('logs', 15, 'btn-icon')}${t('diagnostics.copy')}</button>
        </div>
      </div>

      <div class="grid-2 stagger">
        <div class="card">
          <h3>${icon('cpu', 14)} ${t('diagnostics.system')}</h3>
          ${rows.map(([k, v]) => `
            <div class="kv"><span class="kv-key">${t(k)}</span>
              <span class="kv-val truncate" title="${esc(v)}">${esc(v)}</span></div>`).join('')}
        </div>

        <div class="card">
          <h3>${icon('cpu', 14)} ${t('diagnostics.java')}</h3>
          ${javas.length === 0
            ? `<div class="empty">${t('diagnostics.none')}</div>`
            : javas.map((j) => `
              <div class="kv">
                <span class="kv-key">Java ${j.major}</span>
                <span class="kv-val truncate" title="${esc(j.path)}">${esc(j.path)}</span>
              </div>`).join('')}
        </div>
      </div>

      <div class="card section">
        <h3>${icon('instances', 14)} ${t('diagnostics.workspace')}</h3>
        <div class="grid-3 stagger">
          <div class="stat"><span class="stat-label">${t('diagnostics.instances')}</span>
            <span class="stat-value">${(data.instancesSummary || []).length}</span></div>
          <div class="stat"><span class="stat-label">${t('diagnostics.accounts')}</span>
            <span class="stat-value">${(data.accountsSummary || []).length}</span></div>
          <div class="stat"><span class="stat-label">${t('diagnostics.storage')}</span>
            <span class="stat-value" id="diag-quota">…</span></div>
        </div>
      </div>`;

    this.el.querySelector('#diag-copy')?.addEventListener('click', () => this._copy());
    this._fillQuota();
  }

  async _fillQuota() {
    const cell = this.el.querySelector('#diag-quota');
    if (!cell) return;
    try {
      const est = await navigator.storage?.estimate?.();
      cell.textContent = est?.quota
        ? `${(est.usage / 1048576).toFixed(1)} / ${(est.quota / 1073741824).toFixed(2)} GB`
        : '—';
    } catch {
      cell.textContent = '—';
    }
  }

  async _copy() {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(this._report);
      } else {
        const ta = document.createElement('textarea');
        ta.value = this._report;
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        ta.remove();
      }
      toastSuccess(t('diagnostics.copied'));
    } catch {
      toastError(t('common.error'));
    }
  }

  unmount() {}
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
