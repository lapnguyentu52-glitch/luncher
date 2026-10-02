/**
 * Backup & Restore Center (spec 3.0 mục 22, 74).
 *
 * - Tạo snapshot: chọn instance (hoặc chỉ config) + targets.
 * - List snapshots: label, targets, size, thời gian + actions
 *   Restore / Export ZIP / Delete. Restore LUÔN confirm và tự tạo
 *   pre-restore snapshot phía backend (mục 74).
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { confirmDialog, openModal } from '../../app/modal.js';
import { onBackendEvent } from '../../app/events.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const fmtBytes = (b) => {
  if (!b || b <= 0) return '0 B';
  if (b >= 1024 ** 2) return `${(b / 1024 ** 2).toFixed(1)} MB`;
  if (b >= 1024) return `${(b / 1024).toFixed(0)} KB`;
  return `${b} B`;
};

const TARGET_KEYS = ['instance', 'config', 'mods', 'resourcepacks', 'saves'];

export class BackupsView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('bk.title')}</div>
          <div class="view-sub">${t('bk.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn primary" id="bk-new">${icon('plus', 15, 'btn-icon')}${t('bk.create')}</button>
        </div>
      </div>
      <div class="card section" style="margin-top:0">
        <h3>${icon('database', 14)} ${t('bk.listTitle')}</h3>
        <div id="bk-list" class="list">${t('common.loading')}</div>
      </div>`;

    this.el.querySelector('#bk-new').addEventListener('click', () => this._createDialog());

    this._unsubs.push(
      onBackendEvent('backup.created', () => this._refresh()),
      onBackendEvent('backup.restored', () => this._refresh()),
    );

    await this._refresh();
  }

  async _refresh() {
    const res = await call('backups_list');
    if (!res?.ok) { handleError(res.error); return; }
    const list = res.data.snapshots || [];
    const wrap = this.el.querySelector('#bk-list');
    if (!wrap) return;

    wrap.innerHTML = list.length ? list.map((s) => `
      <div class="row" data-id="${esc(s.id)}">
        <span class="grow truncate">
          <b>${esc(s.label || s.id)}</b>
          <span class="muted"> · ${(s.targets || []).map((x) => esc(t('bk.target.' + x))).join(', ')}</span>
        </span>
        <span class="muted">${s.files || 0} ${t('rs.files')} · ${fmtBytes(s.bytes)}</span>
        <span class="muted">${new Date((s.createdAt || 0) * 1000).toLocaleString()}</span>
        <button class="btn sm" data-act="restore">${icon('refresh', 13, 'btn-icon')}${t('bk.restore')}</button>
        <button class="btn sm ghost" data-act="export">${icon('upload', 13, 'btn-icon')}${t('bk.export')}</button>
        <button class="icon-btn plain" data-act="delete" title="${esc(t('common.delete'))}">${icon('trash', 14)}</button>
      </div>`).join('')
      : `<div class="empty"><div class="big">${icon('database', 26)}</div>
         <div class="empty-title">${t('bk.none')}</div><div>${t('bk.noneSub')}</div></div>`;

    wrap.querySelectorAll('[data-act]').forEach((btn) => {
      btn.addEventListener('click', async () => {
        const id = btn.closest('[data-id]')?.dataset.id;
        if (!id) return;
        if (btn.dataset.act === 'restore') return this._restore(id);
        if (btn.dataset.act === 'export') return this._export(id);
        if (btn.dataset.act === 'delete') return this._delete(id);
      });
    });
  }

  async _createDialog() {
    const instances = await call('instances_list');
    const list = instances?.ok ? instances.data.instances : [];

    const m = openModal({
      title: t('bk.createTitle'),
      icon: 'database',
      body: `
        <div class="field"><label>${t('opt.instance')}</label>
          <select id="bk-inst">
            <option value="">${t('bk.configOnly')}</option>
            ${list.map((i) => `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join('')}
          </select></div>
        <div class="field"><label>${t('bk.targets')}</label>
          <div class="opt-profiles" id="bk-targets">
            ${TARGET_KEYS.map((k, i) => `
              <label class="chip" style="cursor:pointer">
                <input type="checkbox" value="${k}" ${i === 0 ? 'checked' : ''} style="width:auto;margin-right:6px" />
                ${esc(t('bk.target.' + k))}
              </label>`).join('')}
          </div></div>
        <div class="field"><label>${t('bk.label')}</label>
          <input type="text" id="bk-label" placeholder="${esc(t('bk.labelPlaceholder'))}" /></div>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('bk.create'), variant: 'primary', autofocus: true,
          onClick: async ({ close }) => {
            const instId = m.el.querySelector('#bk-inst')?.value || null;
            const targets = [...m.el.querySelectorAll('#bk-targets input:checked')]
              .map((c) => c.value);
            const label = m.el.querySelector('#bk-label')?.value?.trim() || '';
            if (!targets.length) { toastError(t('bk.error.noTarget')); return; }
            const res = await call('backups_create', instId, targets, label);
            if (!res?.ok) { handleError(res.error); return; }
            close();
            toastSuccess(t('bk.toast.created', { id: res.data.snapshot.id }));
          },
        },
      ],
    });
  }

  async _restore(id) {
    const ok = await confirmDialog({
      title: t('bk.restoreConfirmTitle'),
      message: t('bk.restoreConfirmMsg'),
      danger: true,
    });
    if (!ok) return;
    const res = await call('backups_restore', id);
    if (!res?.ok) { handleError(res.error); return; }
    toastSuccess(t('bk.toast.restored', { count: res.data.restored }));
  }

  async _export(id) {
    const res = await call('backups_export_path', id);
    if (!res?.ok) { handleError(res.error); return; }
    toastSuccess(t('bk.toast.exported', { size: fmtBytes(res.data.bytes) }));
  }

  async _delete(id) {
    const ok = await confirmDialog({
      title: t('bk.deleteTitle'),
      message: t('bk.deleteMsg'),
    });
    if (!ok) return;
    const res = await call('backups_delete', id);
    if (!res?.ok) { handleError(res.error); return; }
    toastSuccess(t('bk.toast.deleted'));
    this._refresh();
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
  }
}
