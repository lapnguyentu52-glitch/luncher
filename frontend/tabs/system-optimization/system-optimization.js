/**
 * System Optimization tab (spec 3.0 mục 4, 35-37).
 *
 * - Overview: hardware + battery + power plan (hint-first, không tự áp — mục 35).
 * - Cleanup: chỉ nhóm "Safe" (launcher-owned), chọn nhóm -> clean move-to-trash
 *   -> nút Undo. Protected (saves/mods/...) chỉ hiển thị kích thước (mục 37).
 * - Priority/affinity: hiển thị policy; action áp cho process launcher-owned.
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

const fmtBytes = (b) => {
  if (!b || b <= 0) return '0 B';
  if (b >= 1024 ** 3) return `${(b / 1024 ** 3).toFixed(2)} GB`;
  if (b >= 1024 ** 2) return `${(b / 1024 ** 2).toFixed(1)} MB`;
  if (b >= 1024) return `${(b / 1024).toFixed(0)} KB`;
  return `${b} B`;
};

export class SystemOptimizationView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._overview = null;
    this._selected = new Set();
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('sys.title')}</div>
          <div class="view-sub">${t('sys.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="sys-refresh">${icon('refresh', 15, 'btn-icon')}${t('opt.scan')}</button>
        </div>
      </div>

      <div class="sys-grid">
        <div class="card section" style="margin-top:0">
          <h3>${icon('cpu', 14)} ${t('sys.machine')}</h3>
          <div id="sys-machine" class="opt-machine muted">${t('common.loading')}</div>
        </div>

        <div class="card section" style="margin-top:0">
          <h3>${icon('power', 14)} ${t('sys.power')}</h3>
          <div id="sys-power"></div>
        </div>
      </div>

      <div class="card section">
        <h3>${icon('eraser', 14)} ${t('sys.cleanup')}</h3>
        <div class="muted" style="margin-bottom:10px">${t('sys.cleanupHint')}</div>
        <div id="sys-safe" class="sys-clean-list"></div>
        <div class="modal-foot">
          <button class="btn ghost" id="sys-undo">${icon('refresh', 14, 'btn-icon')}${t('sys.undo')}</button>
          <button class="btn danger" id="sys-clean" disabled>${icon('trash', 14, 'btn-icon')}${t('sys.cleanSelected')}</button>
        </div>
      </div>

      <div class="card section">
        <h3>${icon('shieldCheck', 14)} ${t('sys.protected')}</h3>
        <div class="muted" style="margin-bottom:10px">${t('sys.protectedHint')}</div>
        <div id="sys-protected" class="sys-protected"></div>
      </div>`;

    this.el.querySelector('#sys-refresh').addEventListener('click', () => this._refresh());
    this.el.querySelector('#sys-clean').addEventListener('click', () => this._clean());
    this.el.querySelector('#sys-undo').addEventListener('click', () => this._undoPrompt());
    this.el.querySelector('#sys-safe').addEventListener('change', (e) => {
      const cb = e.target.closest('input[type="checkbox"]');
      if (!cb) return;
      if (cb.checked) this._selected.add(cb.dataset.group);
      else this._selected.delete(cb.dataset.group);
      this._updateCleanButton();
    });

    this._unsubs.push(onBackendEvent('system.cleaned', () => this._refresh()));

    await this._refresh();
  }

  async _refresh() {
    const res = await call('system_overview');
    if (!res?.ok) { handleError(res.error); return; }
    this._overview = res.data;
    this._selected.clear();
    this._renderMachine();
    this._renderPower();
    this._renderCleanup();
    this._updateCleanButton();
  }

  _renderMachine() {
    const box = this.el.querySelector('#sys-machine');
    if (!box) return;
    const hw = this._overview.hardware || {};
    const parts = [];
    if (hw.ramTotalMb) {
      parts.push(`${icon('memory', 13)} ${fmtBytes(hw.ramUsedMb * 1024 * 1024)} / ${fmtBytes(hw.ramTotalMb * 1024 * 1024)} (${hw.ramPercent || 0}%)`);
    }
    parts.push(`${icon('cpu', 13)} ${hw.cpuThreads || '—'} ${t('opt.threads')}`);
    if (hw.battery) {
      parts.push(`${t('sys.battery')} ${Math.round(hw.battery.percent)}%${hw.battery.plugged ? ' ⚡' : ''}`);
    }
    box.innerHTML = parts.join('<span class="opt-sep">·</span>');
  }

  _renderPower() {
    const box = this.el.querySelector('#sys-power');
    if (!box) return;
    const p = this._overview.power || {};
    if (!p.supported) {
      box.innerHTML = `<span class="muted">${t('sys.powerUnsupported')}</span>`;
      return;
    }
    const planName = p.plan?.labelKey ? t('sys.plan.' + p.plan.labelKey) : (p.plan?.name || '—');
    let html = `<div class="row"><span class="grow">${t('sys.currentPlan')}: <b>${esc(planName)}</b></span></div>`;
    const rec = p.recommendation;
    if (rec) {
      const toName = t('sys.plan.' + rec.toLabelKey);
      html += `
        <div class="row">
          <span class="grow muted">${t('sys.hint.' + rec.reasonKey, { plan: toName })}</span>
          <button class="btn sm" id="sys-power-apply">${t('sys.powerApply')}</button>
        </div>`;
    }
    box.innerHTML = html;
    box.querySelector('#sys-power-apply')?.addEventListener('click', async () => {
      const ok = await confirmDialog({
        title: t('sys.powerConfirmTitle'),
        message: t('sys.powerConfirmMsg'),
        danger: false,
      });
      if (!ok) return;
      const r = await call('system_power_set_plan', rec.to);
      if (!r?.ok) { handleError(r.error); return; }
      toastSuccess(t('sys.powerApplied'));
      this._refresh();
    });
  }

  _renderCleanup() {
    const wrap = this.el.querySelector('#sys-safe');
    if (!wrap) return;
    const cp = this._overview.cleanupPreview || {};
    const groups = cp.safe || [];
    wrap.innerHTML = groups.length
      ? groups.map((g) => `
        <label class="row sys-clean-row" data-group-row="${esc(g.id)}">
          <input type="checkbox" data-group="${esc(g.id)}"
                 ${g.count ? '' : 'disabled'} />
          <span class="grow">
            <b>${esc(t('sys.clean.' + g.labelKey))}</b>
            <span class="muted"> · ${g.count} ${t('sys.files')} · ${fmtBytes(g.bytes)}</span>
          </span>
          <span class="badge online">${t('sys.reversible')}</span>
        </label>`).join('')
      : `<div class="empty">${icon('check', 22)}<div>${t('sys.nothingToClean')}</div></div>`;
  }

  _renderProtected() {
    const wrap = this.el.querySelector('#sys-protected');
    if (!wrap) return;
    const prot = this._overview.cleanupPreview?.protected || [];
    wrap.innerHTML = prot.map((p) => `
      <div class="row">
        <span class="grow">${esc(t('sys.prot.' + p.labelKey))}</span>
        <span class="muted">${fmtBytes(p.bytes)}</span>
        <span class="badge">${t('sys.protectedBadge')}</span>
      </div>`).join('');
  }

  _updateCleanButton() {
    const btn = this.el.querySelector('#sys-clean');
    if (btn) btn.disabled = this._selected.size === 0;
  }

  async _clean() {
    const groups = [...this._selected];
    if (!groups.length) return;

    const cp = this._overview?.cleanupPreview || {};
    const paths = [];
    let total = 0;
    for (const g of cp.safe || []) {
      if (!groups.includes(g.id)) continue;
      total += g.bytes || 0;
      for (const it of g.items || []) paths.push(it.path);
    }
    if (!paths.length) return;

    const ok = await confirmDialog({
      title: t('sys.cleanConfirmTitle'),
      message: t('sys.cleanConfirmMsg', { count: paths.length, size: fmtBytes(total) }),
      danger: false,
    });
    if (!ok) return;

    const res = await call('system_cleanup_clean', paths);
    if (!res?.ok) { handleError(res.error); return; }
    this._lastCleanId = res.data.cleanId;
    toastSuccess(t('sys.toast.cleaned', { size: fmtBytes(res.data.bytes) }));
    this._selected.clear();
    await this._refresh();
  }

  async _undoPrompt() {
    if (!this._lastCleanId) {
      toastSuccess(t('sys.toast.nothingToUndo'));
      return;
    }
    const res = await call('system_cleanup_undo', this._lastCleanId);
    if (!res?.ok) { handleError(res.error); return; }
    toastSuccess(t('sys.toast.undone', { count: res.data.restored }));
    this._lastCleanId = null;
    this._refresh();
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
  }
}
