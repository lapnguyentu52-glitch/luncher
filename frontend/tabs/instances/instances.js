/** Instances tab — list + filter + create; grid ảo hoá cho hàng nghìn bản cài (mục 16, 190). */
import { call } from '../../app/bridge.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { openModal, confirmDialog } from '../../app/modal.js';
import { createVirtualList } from '../../app/virtual-list.js';
import { onBackendEvents } from '../../app/events.js';

const LOADERS = ['vanilla', 'fabric', 'forge', 'quilt', 'neoforge'];

/**
 * Chiều cao 1 card + khoảng cách — phải ≥ chiều cao thật của .inst-card
 * (padding 16×2 + inst-top 39 + gap 12 + inst-foot 26 ≈ 109) để không tràn hàng.
 */
const CARD_H = 112;
const CARD_GAP = 16;
const CARD_MIN_W = 230;

export class InstancesView {
  constructor(el) {
    this.el = el;
    this._instances = [];
    this._visible = [];
    this._selected = null;
    this._filter = 'all';
    this._query = '';
    this._list = null;
    this._unsubs = [];
    this._reloadTimer = null;
  }

  async mount() {
    // Remount nội bộ (sau khi xoá instance): gỡ subscription cũ trước.
    this._unsubs.forEach((off) => off());
    this._unsubs = [];

    const [res, state] = await Promise.all([call('instances_list'), call('app_get_state')]);
    this._instances = res?.ok ? (res.data.instances || []) : [];
    this._selected = state?.ok ? state.data.selectedInstance : null;

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('instances.title')}</div>
          <div class="view-sub">${t('instances.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="inst-import">${icon('upload', 15, 'btn-icon')}${t('instances.import')}</button>
          <button class="btn primary" id="inst-new">${icon('plus', 15, 'btn-icon')}${t('instances.new')}</button>
          <input type="file" id="inst-mrpack" accept=".mrpack" class="hidden" />
        </div>
      </div>

      <div class="toolbar">
        <input type="text" id="inst-search" class="grow"
               placeholder="${esc(t('instances.searchPlaceholder'))}" value="${esc(this._query)}" />
        <div class="seg" id="inst-filter">
          ${['all', ...LOADERS].map((l) => `
            <button data-filter="${l}" class="${this._filter === l ? 'active' : ''}">
              ${l === 'all' ? t('common.all') : l}
            </button>`).join('')}
        </div>
      </div>

      <div class="row-between" style="margin-bottom:10px">
        <span class="vlist-count" id="inst-count"></span>
      </div>

      <div id="inst-list" data-anim="stagger"></div>`;

    const box = this.el.querySelector('#inst-list');
    box.addEventListener('click', (e) => this._onClick(e));
    box.addEventListener('keydown', (e) => this._onKeydown(e));

    // Grid ảo hoá: cuộn theo #content của shell, chỉ mount các hàng đang nhìn thấy
    this._list?.destroy();
    this._list = createVirtualList({
      mount: box,
      scroller: document.getElementById('content'),
      rowHeight: CARD_H,
      gap: CARD_GAP,
      minColumnWidth: CARD_MIN_W,
      emptyHtml: `<div class="big">${icon('instances', 26)}</div><div class="empty-title">${t('instances.none')}</div>`,
      // UIX v3: renderRow gắn data-anim — hàng mới mount có fade-slide-up
      renderRow: (slice) => slice.map((i) => this._card(i, true)).join(''),
      onRender: (root, info) => {
        const label = this.el.querySelector('#inst-count');
        if (label) label.textContent = t('common.showing', { shown: info.total, total: this._instances.length });
      },
    });

    this.el.querySelector('#inst-new')?.addEventListener('click', () => this._dialog());
    this.el.querySelector('#inst-import')?.addEventListener('click', () => {
      this.el.querySelector('#inst-mrpack')?.click();
    });
    this.el.querySelector('#inst-mrpack')?.addEventListener('change', (e) => this._importMrpack(e));
    this.el.querySelector('#inst-search')?.addEventListener('input', (e) => {
      this._query = e.target.value.trim().toLowerCase();
      this._applyFilter();
    });
    this.el.querySelectorAll('[data-filter]').forEach((b) =>
      b.addEventListener('click', () => {
        this._filter = b.dataset.filter;
        this.el.querySelectorAll('[data-filter]').forEach((x) => x.classList.toggle('active', x === b));
        this._applyFilter();
      }));

    this._applyFilter();

    // instance.created/deleted/updated/modpack -> nạp lại list (debounce 150ms)
    this._unsubs.push(
      onBackendEvents(['instances.changed', 'instance.created', 'instance.deleted'], () => {
        this._reloadSoon();
      }),
    );
  }

  /** Gộp burst event (vd. cài modpack tạo nhiều thay đổi) thành 1 lần nạp. */
  _reloadSoon() {
    if (this._reloadTimer) clearTimeout(this._reloadTimer);
    this._reloadTimer = setTimeout(async () => {
      this._reloadTimer = null;
      const [res, state] = await Promise.all([call('instances_list'), call('app_get_state')]);
      if (!res?.ok || !this.el?.isConnected) return;
      this._instances = res.data.instances || [];
      this._selected = state?.ok ? state.data.selectedInstance : this._selected;
      this._applyFilter();
    }, 150);
  }

  _applyFilter() {
    this._visible = this._instances.filter((i) => {
      if (this._filter !== 'all' && (i.loader || 'vanilla') !== this._filter) return false;
      if (!this._query) return true;
      return `${i.name} ${i.minecraftVersion || ''}`.toLowerCase().includes(this._query);
    });
    this._list?.setItems(this._visible);
  }

  _card(i, anim = false) {
    const active = i.id === this._selected;
    return `
      <div class="inst-card ${active ? 'selected' : ''}" data-id="${esc(i.id)}" role="button" tabindex="0"
           ${anim ? 'data-anim="row"' : ''}>
        <div class="inst-top">
          <span class="inst-mark">${icon('instances', 18)}</span>
          <span class="grow">
            <div class="inst-name truncate">${esc(i.name)}</div>
            <div class="muted truncate">${esc(i.minecraftVersion || '?')} · ${i.memory?.maxMb || 2048} MB</div>
          </span>
        </div>
        <div class="inst-foot">
          <span class="badge accent">${esc(i.loader || 'vanilla')}</span>
          ${active ? `<span class="badge online">${icon('check', 12)}</span>` : ''}
          <span class="grow"></span>
          <button class="btn danger-ghost sm" data-del="${esc(i.id)}" aria-label="${esc(t('common.delete'))}" data-tip="${esc(t('common.delete'))}">
            ${icon('trash', 15)}
          </button>
        </div>
      </div>`;
  }

  async _onClick(e) {
    const del = e.target.closest('[data-del]');
    if (del) {
      e.stopPropagation();
      await this._remove(del.dataset.del);
      return;
    }
    const card = e.target.closest('[data-id]');
    if (!card) return;
    await this._select(card.dataset.id);
  }

  async _onKeydown(e) {
    if (e.key !== 'Enter' && e.key !== ' ') return;
    const card = e.target.closest('[data-id]');
    if (!card) return;
    e.preventDefault();
    await this._select(card.dataset.id);
  }

  async _select(id) {
    const r = await call('instances_select', id);
    if (!r.ok) { handleError(r.error); return; }
    this._selected = id;
    this._list?.refresh();
  }

  async _remove(id) {
    const ok = await confirmDialog({
      title: t('instances.delete'),
      message: t('instances.confirmDelete'),
      confirmText: t('common.delete'),
    });
    if (!ok) return;
    const r = await call('instances_delete', id, true);
    if (!r.ok) { handleError(r.error); return; }
    toastSuccess(t('instances.deleted'));
    this.mount();
  }

  /**
   * Import modpack .mrpack (Modrinth): hỏi tạo instance mới rồi cài vào đó.
   * Backend: read_mrpack_info (tên/mc/loader) + install_mrpack có sha1 + path guard.
   */
  async _importMrpack(e) {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;

    const name = file.name.replace(/\.mrpack$/i, '');
    const res = await call('instances_create', name, '1.21', 'fabric', 2048);
    if (!res.ok) { handleError(res.error); return; }
    const inst = res.data.instance;

    // Backend đọc loader + mc version thật từ modrinth.index.json khi cài
    const task = await call('modpack_install', file.path || file.name, inst.id);
    if (!task.ok) { handleError(task.error); this.mount(); return; }
    toast(t('instances.importing', { name }), 'info');
    this.mount();
  }

  _dialog() {
    const modal = openModal({
      title: t('instances.createTitle'),
      icon: 'instances',
      tone: 'accent',
      body: `
        <div class="field">
          <label for="ni-name">${t('instances.name')}</label>
          <input type="text" id="ni-name" placeholder="${esc(t('instances.namePlaceholder'))}" />
        </div>
        <div class="grid-2">
          <div class="field">
            <label for="ni-loader">${t('instances.loader')}</label>
            <select id="ni-loader">${LOADERS.map((l) => `<option value="${l}">${l}</option>`).join('')}</select>
          </div>
          <div class="field">
            <label for="ni-version">${t('instances.version')}</label>
            <input type="text" id="ni-version" value="1.21" />
          </div>
        </div>
        <div class="field">
          <label for="ni-ram">${t('instances.ram')}</label>
          <input type="text" id="ni-ram" value="2048" />
        </div>`,
      actions: [
        { label: t('accounts.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('instances.create'),
          variant: 'success',
          icon: 'check',
          onClick: async ({ close }) => {
            const name = modal.el.querySelector('#ni-name').value.trim();
            const version = modal.el.querySelector('#ni-version').value.trim();
            const loader = modal.el.querySelector('#ni-loader').value;
            const ram = parseInt(modal.el.querySelector('#ni-ram').value, 10) || 2048;
            if (!name || !version) { toastError(t('instances.error.nameVersion')); return; }
            const res = await call('instances_create', name, version, loader, ram);
            if (!res.ok) { handleError(res.error); return; }
            toastSuccess(t('instances.created', { name }));
            close();
            this.mount();
          },
        },
      ],
    });
    requestAnimationFrame(() => modal.el.querySelector('#ni-name')?.focus());
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    if (this._reloadTimer) clearTimeout(this._reloadTimer);
    this._reloadTimer = null;
    this._list?.destroy();
    this._list = null;
  }
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
