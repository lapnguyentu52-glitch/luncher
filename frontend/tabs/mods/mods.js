/** Mods tab — Modrinth search + install; 2 danh sách đều ảo hoá (mục 16, 147, 192). */
import { call } from '../../app/bridge.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { confirmDialog } from '../../app/modal.js';
import { createVirtualList } from '../../app/virtual-list.js';
import { onBackendEvents } from '../../app/events.js';

/**
 * Cao độ hàng — phải ≥ chiều cao thật của .mod-card (≈ 61) và .row (≈ 46)
 * để card không tràn sang hàng kế tiếp khi hàng cao cố định.
 */
const RESULT_ROW_H = 64;
const RESULT_GAP = 8;
const INSTALLED_ROW_H = 46;

export class ModsView {
  constructor(el) {
    this.el = el;
    this._inst = null;
    this._instances = [];
    this._hits = [];
    this._installed = [];
    this._installing = new Set();
    this._resultList = null;
    this._installedList = null;
    this._unsubs = [];
  }

  async mount() {
    const state = await call('app_get_state');
    this._instances = state?.ok ? (state.data.instancesSummary || []) : [];
    const selected = this._instances.find((i) => i.id === state.data.selectedInstance)
      || this._instances[0] || null;
    this._inst = selected;

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('mods.title')}</div>
          <div class="view-sub">${t('mods.subtitle')}</div>
        </div>
        <div class="page-actions">
          <div class="field" style="margin:0;min-width:240px">
            <select id="mods-inst">
              ${this._instances.length === 0
                ? `<option value="">${t('play.noInstance')}</option>`
                : this._instances.map((i) => `<option value="${esc(i.id)}" ${i === selected ? 'selected' : ''}>
                    ${esc(i.name)} (${esc(i.loader || 'vanilla')} ${esc(i.version || '')})</option>`).join('')}
            </select>
          </div>
        </div>
      </div>

      <div class="toolbar">
        <input type="text" id="mods-q" class="grow" placeholder="${esc(t('mods.searchPlaceholder'))}" />
        <button class="btn primary" id="mods-search">${icon('search', 15, 'btn-icon')}${t('mods.search')}</button>
      </div>

      <div class="card section">
        <div class="row-between" style="margin-bottom:10px">
          <h3 style="margin:0">${icon('search', 14)} ${t('mods.results')}</h3>
          <span class="vlist-count" id="mods-result-count"></span>
        </div>
        <div id="mods-results"></div>
      </div>

      <div class="card section">
        <div class="row-between" style="margin-bottom:10px">
          <h3 style="margin:0">${icon('mods', 14)} ${t('mods.installed')}</h3>
          <span class="vlist-count" id="mods-installed-count"></span>
        </div>
        <div id="mods-installed"></div>
      </div>`;

    const resultBox = this.el.querySelector('#mods-results');
    const installedBox = this.el.querySelector('#mods-installed');
    const scroller = document.getElementById('content');

    // Điều hướng sự kiện uỷ quyền: mount giữ nguyên identity qua mọi lần vẽ lại
    resultBox.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-install]');
      if (btn) this._install(btn);
    });
    installedBox.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-rm]');
      if (btn) this._remove(btn.dataset.rm);
    });

    this._resultList?.destroy();
    this._installedList?.destroy();

    this._resultList = createVirtualList({
      mount: resultBox,
      scroller,
      rowHeight: RESULT_ROW_H,
      gap: RESULT_GAP,
      emptyHtml: `<div class="big">${icon('search', 26)}</div>${t('mods.selectInstance')}`,
      renderRow: (slice) => slice.map((h) => this._resultCard(h)).join(''),
      onRender: (root, info) => this._setCount('#mods-result-count', info.total, this._hits.length),
    });

    this._installedList = createVirtualList({
      mount: installedBox,
      scroller,
      rowHeight: INSTALLED_ROW_H,
      gap: 0,
      rowClass: 'vlist-sep-row',
      emptyHtml: t('mods.noInstalled'),
      renderRow: (slice) => slice.map((m) => this._installedRow(m)).join(''),
      onRender: (root, info) => this._setCount('#mods-installed-count', info.total, this._installed.length),
    });

    this.el.querySelector('#mods-search')?.addEventListener('click', () => this._search());
    this.el.querySelector('#mods-q')?.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this._search();
    });
    this.el.querySelector('#mods-inst')?.addEventListener('change', (e) => {
      this._inst = this._instances.find((i) => i.id === e.target.value) || null;
      this._installed = [];
      this._installedList.setItems([]);
      if (this._inst) this._refreshInstalled(this._inst.id);
    });

    if (selected) this._refreshInstalled(selected.id);

    // mods.changed (cài/gỡ từ nơi khác, modpack install) -> refresh mod đã cài
    this._unsubs.push(onBackendEvents(['mods.changed'], (e) => {
      const changed = e.payload?.instanceId;
      if (changed && this._inst && changed !== this._inst.id) return;
      if (this._inst) this._refreshInstalled(this._inst.id);
    }));
  }

  _setCount(sel, shown, total) {
    const node = this.el.querySelector(sel);
    if (node) node.textContent = total ? t('common.showing', { shown, total }) : '';
  }

  /* ---------------- Search results ---------------- */

  _resultCard(h) {
    const busy = this._installing.has(String(h.projectId));
    return `
      <div class="mod-card">
        <span class="mod-icon">${esc((h.title || '?')[0])}</span>
        <span class="grow">
          <div class="truncate">${esc(h.title)}</div>
          <div class="muted truncate">${esc(h.author || '')} · ${esc(t('mods.downloadCount', { count: h.downloads ?? 0 }))}</div>
        </span>
        <button class="btn success sm" data-install="${esc(h.projectId)}" ${busy ? 'disabled' : ''}>
          ${busy ? '<span class="spinner"></span>' : `${icon('downloads', 14, 'btn-icon')}${t('mods.install')}`}
        </button>
      </div>`;
  }

  async _search() {
    if (!this._inst) { toastError(t('mods.selectInstance')); return; }
    const query = this.el.querySelector('#mods-q').value.trim();

    this._hits = [];
    this._resultList.setItems([]);
    this._resultList.el.innerHTML = `<div class="vlist-empty"><span class="spinner"></span>${t('mods.searching')}</div>`;

    const res = await call('mods_search', query, this._inst.loader || 'fabric', this._inst.version);
    if (!res.ok) {
      handleError(res.error);
      this._resultList.setItems([]);
      return;
    }
    this._hits = res.data.hits || [];
    this._resultList.setItems(this._hits);
  }

  async _install(btn) {
    const projectId = btn.dataset.install;
    this._installing.add(String(projectId));
    this._resultList.refresh();
    try {
      const r = await call('mods_install', projectId, this._inst.id,
                           this._inst.loader || 'fabric', this._inst.version);
      if (!r.ok) { handleError(r.error); return; }
      toastSuccess(t('mods.toast.installed', { file: r.data.filename }));
      this._refreshInstalled(this._inst.id);
    } finally {
      this._installing.delete(String(projectId));
      this._resultList.refresh();
    }
  }

  /* ---------------- Installed ---------------- */

  _installedRow(name) {
    return `
      <div class="row">
        <span class="badge accent">${icon('mods', 12)}</span>
        <span class="grow truncate">${esc(name)}</span>
        <button class="btn danger-ghost sm" data-rm="${esc(name)}" aria-label="${esc(t('common.delete'))}" data-tip="${esc(t('common.delete'))}">
          ${icon('trash', 15)}
        </button>
      </div>`;
  }

  async _refreshInstalled(instId) {
    if (!instId || !this._installedList) return;
    const res = await call('mods_list_installed', instId);
    this._installed = res?.ok ? (res.data.mods || []) : [];
    this._installedList.setItems(this._installed);
  }

  async _remove(filename) {
    const ok = await confirmDialog({
      title: t('common.delete'),
      message: `${t('accounts.confirmDelete')} ${filename}`,
      confirmText: t('common.delete'),
    });
    if (!ok) return;
    const r = await call('mods_remove', this._inst.id, filename);
    if (!r.ok) { handleError(r.error); return; }
    this._refreshInstalled(this._inst.id);
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._resultList?.destroy();
    this._installedList?.destroy();
    this._resultList = null;
    this._installedList = null;
  }
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
