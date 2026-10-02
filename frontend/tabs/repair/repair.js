/**
 * Repair Center (spec 3.0 mục 39).
 *
 * Flow bắt buộc: chọn action → "Scan" (dry-run: findings + planned steps,
 * KHÔNG ghi gì) → confirm → "Run". File bị dọn đi qua trash (undo được
 * từ System Optimization).
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { confirmDialog } from '../../app/modal.js';
import { onBackendEvent } from '../../app/events.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const ACTION_ICONS = {
  instance_metadata: 'instances',
  missing_dirs: 'folder',
  option_files: 'edit',
  launcher_config: 'settings',
  downloads: 'downloads',
  caches: 'database',
  resource_packs: 'layers',
};

export class RepairView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._scan = null;
    this._instanceId = null;
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('rp.title')}</div>
          <div class="view-sub">${t('rp.subtitle')}</div>
        </div>
      </div>

      <div class="card section" style="margin-top:0">
        <h3>${icon('shieldCheck', 14)} ${t('rp.actions')}</h3>
        <div class="field">
          <label>${t('opt.instance')}</label>
          <select id="rp-instance"><option value="">${t('rp.allInstances')}</option></select>
        </div>
        <div id="rp-actions" class="opt-profiles">${t('common.loading')}</div>
      </div>

      <div class="card section" id="rp-scan-card" hidden>
        <h3>${icon('search', 14)} ${t('rp.scanTitle')}</h3>
        <div id="rp-scan"></div>
        <div class="modal-foot">
          <button class="btn ghost" id="rp-cancel">${t('common.cancel')}</button>
          <button class="btn primary" id="rp-run">${icon('check', 15, 'btn-icon')}${t('rp.run')}</button>
        </div>
      </div>`;

    this.el.querySelector('#rp-instance').addEventListener('change', (e) => {
      this._instanceId = e.target.value || null;
    });
    this.el.querySelector('#rp-actions').addEventListener('click', (e) => {
      const chip = e.target.closest('[data-action]');
      if (chip) this._scanAction(chip.dataset.action);
    });
    this.el.querySelector('#rp-cancel').addEventListener('click', () => {
      const card = this.el.querySelector('#rp-scan-card');
      if (card) card.hidden = true;
    });
    this.el.querySelector('#rp-run').addEventListener('click', () => this._run());

    this._unsubs.push(onBackendEvent('repair.run', () => this._toastRun()));
    this._lastRun = null;

    await this._loadInstances();
    this._loadActions();
  }

  async _loadInstances() {
    const sel = this.el.querySelector('#rp-instance');
    const res = await call('instances_list');
    if (!res?.ok || !sel?.isConnected) return;
    const list = res.data.instances || [];
    sel.insertAdjacentHTML('beforeend', list.map((i) =>
      `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join(''));
  }

  async _loadActions() {
    const res = await call('repair_actions');
    if (!res?.ok) { handleError(res.error); return; }
    const wrap = this.el.querySelector('#rp-actions');
    if (!wrap) return;
    wrap.innerHTML = res.data.actions.map((a) => `
      <button class="chip" data-action="${esc(a)}">
        ${icon(ACTION_ICONS[a] || 'shieldCheck', 13)} ${esc(t('rp.action.' + a))}
      </button>`).join('');
  }

  async _scanAction(action) {
    const res = await call('repair_scan', action, this._instanceId);
    if (!res?.ok) { handleError(res.error); return; }
    this._scan = res.data;
    this._scan.action = action;

    const card = this.el.querySelector('#rp-scan-card');
    const box = this.el.querySelector('#rp-scan');
    if (!card || !box) return;

    const findings = res.data.findings || [];
    const planned = res.data.planned || [];

    box.innerHTML = `
      <div class="muted" style="margin-bottom:8px">
        ${esc(t('rp.scanHint', { action: t('rp.action.' + action) }))}
      </div>
      ${findings.length ? `
        <div class="rp-section">${t('rp.findings')}</div>
        ${findings.map((f) => `
          <div class="row">
            <span class="badge ${f.severity === 'ERROR' ? 'offline' : 'warn'}">${esc(f.severity)}</span>
            <span class="grow truncate" title="${esc(f.path)}">${esc(f.detail)}</span>
            <span class="muted truncate" style="max-width:40%">${esc(f.path)}</span>
          </div>`).join('')}` : ''}
      ${planned.length ? `
        <div class="rp-section">${t('rp.planned')}</div>
        ${planned.map((p) => `
          <div class="row">
            <span class="badge accent">${icon('arrowRight', 12)}</span>
            <span class="grow truncate">${esc(p.detail)}</span>
          </div>`).join('')}` : ''}
      ${!findings.length && !planned.length ? `
        <div class="empty">${icon('check', 24)}<div>${t('rp.healthy')}</div></div>` : ''}`;

    card.hidden = false;
    this.el.querySelector('#rp-run').disabled = !planned.length;
    card.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  async _run() {
    if (!this._scan) return;
    const ok = await confirmDialog({
      title: t('rp.runConfirmTitle'),
      message: t('rp.runConfirmMsg', { count: this._scan.planned.length }),
      danger: false,
    });
    if (!ok) return;
    const res = await call('repair_run', this._scan.action, this._instanceId);
    if (!res?.ok) { handleError(res.error); return; }
    this._lastRun = res.data;
    this.el.querySelector('#rp-scan-card').hidden = true;
    this._toastRun(true);
  }

  _toastRun(explicit = false) {
    if (this._lastRun && explicit) {
      toastSuccess(t('rp.toast.done', { count: this._lastRun.performed }));
      this._lastRun = null;
    }
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
  }
}
