/**
 * Skins tab — Skin & Cape Studio (mục 43): import PNG, generate Steve-style,
 * preview, apply/unapply per-instance. Apply state backend đọc khi launch.
 */
import { call } from '../../app/bridge.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { openModal, confirmDialog } from '../../app/modal.js';
import { onBackendEvents } from '../../app/events.js';

export class SkinsView {
  constructor(el) {
    this.el = el;
    this._kind = 'skin';
    this._items = [];
    this._state = null;          // instancesSummary
    this._previews = new Map();  // id -> data URI
    this._unsubs = [];
  }

  async mount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    const state = await call('app_get_state');
    this._state = state?.ok ? state.data : null;

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('skins.title')}</div>
          <div class="view-sub">${t('skins.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="sk-import">${icon('upload', 15, 'btn-icon')}${t('skins.import')}</button>
          <button class="btn primary" id="sk-gen">${icon('sparkle', 15, 'btn-icon')}${t('skins.generate')}</button>
          <input type="file" id="sk-file" accept=".png" class="hidden" />
        </div>
      </div>

      <div class="toolbar" style="margin-bottom:12px">
        <div class="seg" id="sk-kind">
          <button data-kind="skin" class="active">${t('skins.skins')}</button>
          <button data-kind="cape">${t('skins.capes')}</button>
        </div>
        <span class="grow"></span>
        <select id="sk-instance" style="max-width:220px">
          <option value="">${t('skins.pickInstance')}</option>
          ${(this._state?.instancesSummary || []).map((i) =>
            `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join('')}
        </select>
      </div>

      <div id="sk-list" class="skin-grid stagger"></div>`;

    this.el.querySelector('#sk-import')?.addEventListener('click', () =>
      this.el.querySelector('#sk-file')?.click());
    this.el.querySelector('#sk-file')?.addEventListener('change', (e) => this._importFile(e));
    this.el.querySelector('#sk-gen')?.addEventListener('click', () => this._genDialog());
    this.el.querySelector('#sk-kind')?.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-kind]');
      if (!btn) return;
      this._kind = btn.dataset.kind;
      this.el.querySelectorAll('#sk-kind [data-kind]').forEach((x) =>
        x.classList.toggle('active', x === btn));
      this._refresh();
    });
    this.el.querySelector('#sk-instance')?.addEventListener('change', (e) => {
      this._instanceId = e.target.value || null;
      this._markApplied();
    });

    this._unsubs.push(onBackendEvents(['skins.changed', 'skin.applied'],
      () => this._refresh()));
    await this._refresh();
  }

  _instanceId() {
    const sel = this.el.querySelector('#sk-instance');
    return sel ? (sel.value || null) : null;
  }

  async _refresh() {
    const res = await call('skins_list', this._kind);
    if (!res?.ok) { handleError(res.error); return; }
    this._items = res.data[`${this._kind}s`] || [];
    // preview song song — cache theo id
    await Promise.all(this._items.map(async (item) => {
      if (this._previews.has(`${this._kind}:${item.id}:${item.sha256}`)) return;
      const p = await call('skins_preview', this._kind, item.id);
      if (p?.ok && p.data.preview) {
        this._previews.set(`${this._kind}:${item.id}:${item.sha256}`, p.data.preview);
      }
    }));
    this._render();
    await this._markApplied();
  }

  async _markApplied() {
    const iid = this._instanceId();
    if (!iid) { this._render(); return; }
    const res = await call('skins_applied', iid);
    if (!res?.ok) { this._render(); return; }
    const applied = res.data.applied || {};
    const activeId = applied[this._kind]?.id || null;
    this._render(activeId);
  }

  _render(activeId = null) {
    const box = this.el.querySelector('#sk-list');
    if (!box) return;
    if (!this._items.length) {
      box.innerHTML = `<div class="empty">
        <div class="big">${icon('user', 26)}</div>
        <div class="empty-title">${t('skins.none')}</div>
        <div>${t('skins.noneSub')}</div>
      </div>`;
      box.onclick = null;
      return;
    }
    box.innerHTML = this._items.map((item) => {
      const uri = this._previews.get(`${this._kind}:${item.id}:${item.sha256}`);
      const active = item.id === activeId;
      return `
      <div class="skin-card ${active ? 'selected' : ''}" data-id="${esc(item.id)}">
        <div class="skin-preview">${uri
          ? `<img src="${uri}" alt="${esc(item.name)}" />`
          : icon('user', 32)}</div>
        <div class="skin-meta">
          <div class="skin-name truncate">${esc(item.name)}</div>
          <div class="muted">${item.width}×${item.height}${item.converted ? ' · 64x32→64x64' : ''}</div>
        </div>
        <div class="skin-actions">
          ${active ? `<span class="badge online">${icon('check', 12)}</span>` : ''}
          <span class="grow"></span>
          <button class="btn sm primary" data-apply="${esc(item.id)}">${t('skins.apply')}</button>
          <button class="btn danger-ghost sm" data-del="${esc(item.id)}">${icon('trash', 14)}</button>
        </div>
      </div>`;
    }).join('');

    box.onclick = async (e) => {
      const del = e.target.closest('[data-del]');
      if (del) { await this._remove(del.dataset.del); return; }
      const apply = e.target.closest('[data-apply]');
      if (apply) { await this._apply(apply.dataset.apply); return; }
    };
  }

  async _apply(id) {
    const iid = this._instanceId();
    if (!iid) { toastError(t('skins.needInstance')); return; }
    const r = await call('skins_apply', iid, this._kind, id);
    if (!r.ok) { handleError(r.error); return; }
    toastSuccess(t('skins.applied'));
    this._markApplied();
  }

  async _remove(id) {
    const item = this._items.find((x) => x.id === id);
    const ok = await confirmDialog({
      title: t('skins.deleteTitle'),
      message: t('skins.confirmDelete', { name: item?.name || id }),
      confirmText: t('common.delete'),
    });
    if (!ok) return;
    const r = await call('skins_delete', this._kind, id, true);
    if (!r.ok) { handleError(r.error); return; }
    toastSuccess(t('skins.deleted'));
    this._refresh();
  }

  async _importFile(e) {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    try {
      const b64 = await this._fileToB64(file);
      const name = file.name.replace(/\.png$/i, '').toLowerCase().replace(/[^a-z0-9_-]/g, '_');
      const r = await call('skins_import', b64, name, this._kind);
      if (!r.ok) { handleError(r.error); return; }
      toastSuccess(t('skins.imported', { name: r.data.item.name }));
      this._refresh();
    } catch {
      toastError(t('skins.importFailed'));
    }
  }

  _fileToB64(file) {
    return new Promise((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(String(r.result).split(',', 2)[1] || '');
      r.onerror = () => reject(new Error('read fail'));
      r.readAsDataURL(file);
    });
  }

  /** Wizard tạo skin/cape: màu chính + màu phụ, preview thật bằng backend render. */
  _genDialog() {
    const kind = this._kind;
    const modal = openModal({
      title: kind === 'skin' ? t('skins.genSkinTitle') : t('skins.genCapeTitle'),
      icon: 'sparkle',
      tone: 'accent',
      body: `
        <div class="field">
          <label for="sk-g-name">${t('skins.name')}</label>
          <input type="text" id="sk-g-name" placeholder="${esc(t('skins.namePlaceholder'))}" />
        </div>
        <div class="grid-2">
          <div class="field">
            <label for="sk-g-base">${t('skins.baseColor')}</label>
            <input type="color" id="sk-g-base" value="#c8965a" />
          </div>
          <div class="field">
            <label for="sk-g-accent">${t('skins.accentColor')}</label>
            <input type="color" id="sk-g-accent" value="#3b2a1a" />
          </div>
        </div>
        <p class="muted">${kind === 'skin' ? t('skins.genSkinHint') : t('skins.genCapeHint')}</p>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('skins.generate'), variant: 'success', icon: 'check',
          onClick: async ({ close }) => {
            const name = modal.el.querySelector('#sk-g-name').value.trim();
            const base = modal.el.querySelector('#sk-g-base').value;
            const accent = modal.el.querySelector('#sk-g-accent').value;
            if (!name) { toastError(t('skins.error.name')); return; }
            const r = await call('skins_generate', kind, name, base, accent);
            if (!r.ok) { handleError(r.error); return; }
            close();
            toastSuccess(t('skins.generated', { name: r.data.item.name }));
            this._refresh();
          },
        },
      ],
    });
    requestAnimationFrame(() => modal.el.querySelector('#sk-g-name')?.focus());
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
  }
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
