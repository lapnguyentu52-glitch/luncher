/**
 * Game Optimization tab (spec 3.0 mục 3, 70, 72).
 *
 * Flow đúng spec 3.1: Scan → Recommendations → Preview (diff) → Backup+Apply → Rollback.
 * - Chọn instance + profile -> "Preview changes" hiện diff trước khi ghi (preview > blind apply).
 * - Apply tự tạo snapshot — rollback 1 click sau đó (mục 70).
 * - Không đụng system-wide — chỉ instance.json + game/options.txt (mục 74).
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { onBackendEvent } from '../../app/events.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const fmtVal = (v) => (v === null || v === undefined || v === '' ? '—' : String(v));

export class GameOptimizationView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._scan = null;
    this._plan = null;
    this._instanceId = null;
    this._profileId = 'balanced';
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('opt.title')}</div>
          <div class="view-sub">${t('opt.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="opt-rescan">${icon('refresh', 15, 'btn-icon')}${t('opt.scan')}</button>
        </div>
      </div>

      <div class="opt-layout">
        <div class="card section" style="margin-top:0">
          <h3>${icon('cpu', 14)} ${t('opt.machine')}</h3>
          <div id="opt-machine" class="opt-machine muted">${t('common.loading')}</div>
        </div>

        <div class="card section" style="margin-top:0">
          <h3>${icon('rocket', 14)} ${t('opt.profiles')}</h3>
          <div class="field">
            <label>${t('opt.instance')}</label>
            <select id="opt-instance"></select>
          </div>
          <div id="opt-profiles" class="opt-profiles"></div>
        </div>
      </div>

      <div class="card section" id="opt-preview-card" hidden>
        <h3>${icon('edit', 14)} ${t('opt.previewTitle')}</h3>
        <div id="opt-preview"></div>
        <div class="modal-foot">
          <button class="btn ghost" id="opt-cancel">${t('common.cancel')}</button>
          <button class="btn primary" id="opt-apply">${icon('check', 15, 'btn-icon')}${t('opt.apply')}</button>
        </div>
      </div>

      <div class="card section" id="opt-snapshot-card" hidden>
        <h3>${icon('database', 14)} ${t('opt.snapshotTitle')}</h3>
        <div id="opt-snapshot" class="opt-snapshot"></div>
      </div>`;

    this.el.querySelector('#opt-rescan').addEventListener('click', () => this._scanNow());
    this.el.querySelector('#opt-cancel').addEventListener('click', () => this._hidePreview());
    this.el.querySelector('#opt-apply').addEventListener('click', () => this._apply());

    this.el.querySelector('#opt-instance').addEventListener('change', (e) => {
      this._instanceId = e.target.value || null;
      this._refreshSnapshot();
    });
    this.el.querySelector('#opt-profiles').addEventListener('click', (e) => {
      const chip = e.target.closest('[data-profile]');
      if (chip) {
        this._profileId = chip.dataset.profile;
        this._renderProfiles();
      }
    });
    this.el.querySelector('#opt-profiles').addEventListener('dblclick', (e) => {
      const chip = e.target.closest('[data-profile]');
      if (chip && this._instanceId) this._preview();
    });

    this._unsubs.push(onBackendEvent('optimization.applied', () => {
      this._refreshSnapshot();
    }));

    await this._scanNow();
  }

  /* ---------------------------------------------------------------- */

  async _scanNow() {
    const res = await call('optimization_scan', this._instanceId);
    if (!res?.ok) { handleError(res.error); return; }
    this._scan = res.data;
    this._renderMachine();
    this._renderInstanceSelect();
    this._renderProfiles();
    this._refreshSnapshot();
  }

  _renderMachine() {
    const box = this.el.querySelector('#opt-machine');
    if (!box || !this._scan) return;
    const hw = this._scan.hardware || {};
    const mem = this._scan.memory;
    const parts = [
      `${icon('memory', 13)} ${hw.ramTotalMb ? (hw.ramTotalMb / 1024).toFixed(1) + ' GB RAM' : '—'}`,
      `${icon('cpu', 13)} ${hw.cpuThreads || '—'} ${t('opt.threads')}`,
    ];
    if (mem) parts.push(`${t('opt.recommendHeap')}: <b>${mem.maxMb} MB</b>`);
    if (this._scan.profile) {
      parts.push(`${t('opt.recommendProfile')}: <b>${esc(t('opt.profile.' + this._scan.profile))}</b>`);
    }
    for (const w of this._scan.warnings || []) {
      parts.push(`<span class="badge warn">${esc(t('opt.warn.' + w))}</span>`);
    }
    box.innerHTML = parts.join('<span class="opt-sep">·</span>');
  }

  _renderInstanceSelect() {
    const sel = this.el.querySelector('#opt-instance');
    if (!sel) return;
    call('instances_list').then((res) => {
      if (!res?.ok || !sel.isConnected) return;
      const list = res.data.instances || [];
      sel.innerHTML = list.length
        ? list.map((i) =>
            `<option value="${esc(i.id)}"${i.id === this._instanceId ? ' selected' : ''}>${esc(i.name)}</option>`).join('')
        : `<option value="">${esc(t('opt.noInstances'))}</option>`;
      if (!this._instanceId && list[0]) this._instanceId = list[0].id;
    });
  }

  _renderProfiles() {
    const wrap = this.el.querySelector('#opt-profiles');
    if (!wrap || !this._scan) return;
    wrap.innerHTML = (this._scan.profiles || []).map((p) => `
      <button class="chip${p.id === this._profileId ? ' active' : ''}" data-profile="${esc(p.id)}"
              title="${esc(t('opt.profile.' + p.descKey))}">
        ${esc(t('opt.profile.' + p.labelKey))}
        ${this._scan.profile === p.id ? `<span class="badge accent">${t('opt.recommended')}</span>` : ''}
      </button>`).join('');
  }

  async _preview() {
    if (!this._instanceId || !this._profileId) return;
    const res = await call('optimization_plan', this._instanceId, this._profileId);
    if (!res?.ok) { handleError(res.error); return; }
    this._plan = res.data;
    const card = this.el.querySelector('#opt-preview-card');
    const box = this.el.querySelector('#opt-preview');
    if (!card || !box) return;

    const rows = [];
    const section = (titleKey, changes) => {
      if (!changes.length) return;
      rows.push(`<div class="opt-diff-section">${t(titleKey)}</div>`);
      for (const ch of changes) {
        rows.push(`
          <div class="row opt-diff-row">
            <span class="grow truncate"><b>${esc(ch.field)}</b></span>
            <span class="opt-before">${esc(fmtVal(ch.before))}</span>
            <span class="opt-arrow">${icon('arrowRight', 13)}</span>
            <span class="opt-after">${esc(fmtVal(ch.after))}</span>
          </div>`);
      }
    };
    section('opt.section.jvm', this._plan.jvm);
    section('opt.section.minecraft', this._plan.minecraft);

    box.innerHTML = rows.length
      ? rows.join('')
      : `<div class="empty">${icon('check', 24)}<div>${t('opt.noChanges')}</div></div>`;
    card.hidden = false;
    card.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  _hidePreview() {
    this._plan = null;
    const card = this.el.querySelector('#opt-preview-card');
    if (card) card.hidden = true;
  }

  async _apply() {
    if (!this._plan || !this._instanceId) return;
    const btn = this.el.querySelector('#opt-apply');
    if (btn) btn.disabled = true;
    try {
      const res = await call('optimization_apply', this._instanceId, this._profileId);
      if (!res?.ok) { handleError(res.error); return; }
      toastSuccess(t('opt.toast.applied', { profile: t('opt.profile.' + this._profileId) }));
      this._hidePreview();
      this._refreshSnapshot();
    } finally {
      if (btn) btn.disabled = false;
    }
  }

  async _refreshSnapshot() {
    if (!this._instanceId) { this._toggleSnapshot(null); return; }
    const res = await call('optimization_snapshot_info', this._instanceId);
    if (!res?.ok) return;
    const snap = res.data.snapshot;
    const card = this.el.querySelector('#opt-snapshot-card');
    const box = this.el.querySelector('#opt-snapshot');
    if (!card || !box) return;
    if (!snap) { card.hidden = true; return; }
    const when = snap.appliedAt ? new Date(snap.appliedAt * 1000).toLocaleString() : '—';
    box.innerHTML = `
      <div class="row">
        <span class="grow">
          <b>${esc(t('opt.profile.' + (snap.profile || 'balanced')))}</b>
          <span class="muted">· ${esc(when)}</span>
        </span>
        <button class="btn sm" id="opt-rollback">${icon('refresh', 13, 'btn-icon')}${t('opt.rollback')}</button>
      </div>`;
    card.hidden = false;
    box.querySelector('#opt-rollback')?.addEventListener('click', async () => {
      const r = await call('optimization_rollback', this._instanceId);
      if (!r?.ok) { handleError(r.error); return; }
      toastSuccess(t('opt.toast.rolledBack'));
      card.hidden = true;
      this._scanNow();
    });
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
  }
}
