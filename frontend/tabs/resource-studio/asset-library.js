/**
 * Asset Library UI (master plan v3 mục 12, 50 — Batch 5).
 *
 * Grid ảo hoá qua createVirtualList (tái sử dụng — mục 18: KHÔNG tạo hệ
 * thống list thứ hai). Import: file picker + drag-drop -> base64 -> backend
 * validate/hash/atomic (mục 12.1). Search debounce 250ms + cancel stale
 * (mục 51). Preview panel: checkerboard + metadata + assign + delete.
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { confirmDialog } from '../../app/modal.js';
import { createVirtualList } from '../../app/virtual-list.js';
import { thumbCache } from '../../app/thumb-cache.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const CATEGORIES = ['block', 'item', 'entity', 'gui', 'particle', 'font', 'sound', 'lang'];
const SORTS = ['newest', 'oldest', 'name', 'size'];
const MAX_FILE_BYTES = 8 * 1024 * 1024;

export class AssetLibraryView {
  constructor(el) {
    this.el = el;
    this._assets = [];
    this._selected = null;
    this._vlist = null;
    this._searchTimer = 0;
    this._searchSeq = 0;        // cancel stale search (mục 18)
    this._query = '';
    this._category = '';
    this._sort = 'newest';
    this._unsubs = [];
  }

  async mount() {
    this.el.innerHTML = `
      <div class="al-toolbar">
        <div class="al-search">
          ${icon('search', 14)}
          <input type="search" id="al-search" placeholder="${esc(t('al.searchPlaceholder'))}"
                 aria-label="${esc(t('al.search'))}" />
        </div>
        <select id="al-category" aria-label="${esc(t('al.category'))}">
          <option value="">${esc(t('al.allCategories'))}</option>
          ${CATEGORIES.map((c) => `<option value="${c}">${esc(t('al.cat.' + c))}</option>`).join('')}
        </select>
        <select id="al-sort" aria-label="${esc(t('al.sort'))}">
          ${SORTS.map((s) => `<option value="${s}">${esc(t('al.sort.' + s))}</option>`).join('')}
        </select>
        <button class="btn primary" id="al-import">${icon('upload', 14, 'btn-icon')}${esc(t('al.import'))}</button>
        <input type="file" id="al-file" accept=".png,image/png" multiple hidden />
      </div>
      <div class="al-cols">
        <div id="al-grid" class="al-grid"></div>
        <div id="al-preview" class="card section al-preview"></div>
      </div>`;

    this._bindToolbar();
    this._bindDrop();

    this._vlist = createVirtualList({
      mount: this.el.querySelector('#al-grid'),
      scroller: null,
      rowHeight: 148,
      gap: 10,
      minColumnWidth: 132,
      renderRow: (items, start) => items.map((a, i) => this._card(a, start + i)).join(''),
      emptyHtml: `<div class="empty"><div class="big">${icon('copy', 26)}</div>
        <div class="empty-title">${esc(t('al.emptyTitle'))}</div>
        <div>${esc(t('al.emptySub'))}</div></div>`,
      onRender: (root) => this._observeThumbs(root),
    });
    this._unsubs.push(() => { this._vlist?.destroy(); this._vlist = null; });

    // Delegation: click + keyboard trên grid (re-render của vlist không mất listener)
    const grid = this.el.querySelector('#al-grid');
    const onClick = (e) => this._onGridClick(e);
    const onKey = (e) => {
      if (e.key !== 'Enter' && e.key !== ' ') return;
      const card = e.target.closest?.('.al-card');
      if (card) { e.preventDefault(); this._selected = card.dataset.id; this._refreshGridSelection(); this._renderPreview(); }
    };
    grid.addEventListener('click', onClick);
    grid.addEventListener('keydown', onKey);
    this._unsubs.push(() => {
      grid.removeEventListener('click', onClick);
      grid.removeEventListener('keydown', onKey);
    });

    await this._refresh();
  }

  unmount() {
    if (this._searchTimer) { clearTimeout(this._searchTimer); this._searchTimer = 0; }
    this._thumbIO?.disconnect();
    this._thumbIO = null;
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._assets = [];
    // Cache singleton giữ lại qua tab switch (mục 79) — chỉ clear khi xoá asset
  }

  /* ================= toolbar ================= */

  _bindToolbar() {
    const search = this.el.querySelector('#al-search');
    search.addEventListener('input', () => {
      if (this._searchTimer) clearTimeout(this._searchTimer);
      this._searchTimer = setTimeout(() => {
        this._searchTimer = 0;
        this._query = search.value;
        this._refresh();
      }, 250);
    });

    this.el.querySelector('#al-category').addEventListener('change', (e) => {
      this._category = e.target.value;
      this._refresh();
    });
    this.el.querySelector('#al-sort').addEventListener('change', (e) => {
      this._sort = e.target.value;
      this._refresh();
    });

    const file = this.el.querySelector('#al-file');
    this.el.querySelector('#al-import').addEventListener('click', () => file.click());
    file.addEventListener('change', () => {
      this._importFiles([...file.files]);
      file.value = '';
    });
  }

  _bindDrop() {
    const grid = this.el.querySelector('#al-grid');
    const dragOver = (e) => { e.preventDefault(); grid.classList.add('al-drag'); };
    const dragLeave = () => grid.classList.remove('al-drag');
    const drop = (e) => {
      e.preventDefault();
      grid.classList.remove('al-drag');
      const files = [...(e.dataTransfer?.files || [])].filter((f) =>
        f.type === 'image/png' || f.name?.toLowerCase().endsWith('.png'));
      if (files.length) this._importFiles(files);
      else if (e.dataTransfer?.files?.length) toastError(t('al.error.notPng'));
    };
    grid.addEventListener('dragover', dragOver);
    grid.addEventListener('dragleave', dragLeave);
    grid.addEventListener('drop', drop);
    this._unsubs.push(() => {
      grid.removeEventListener('dragover', dragOver);
      grid.removeEventListener('dragleave', dragLeave);
      grid.removeEventListener('drop', drop);
    });
  }

  async _importFiles(files) {
    // mục 51: 1 toast tổng, không spam
    let ok = 0;
    const errors = [];
    for (const f of files.slice(0, 50)) {
      if (f.size > MAX_FILE_BYTES) { errors.push(`${f.name}: >8MB`); continue; }
      try {
        const b64 = await this._fileToB64(f);
        const res = await call('asset_import', b64,
          f.name.replace(/\.png$/i, ''), 'item', []);
        if (res?.ok) ok++;
        else errors.push(`${f.name}: ${res?.error?.message || 'fail'}`);
      } catch {
        errors.push(`${f.name}: read error`);
      }
    }
    if (ok) toastSuccess(t('al.toast.imported', { count: ok }));
    if (errors.length) toastError(`${errors.length} lỗi: ${errors[0]}${errors.length > 1 ? '…' : ''}`);
    if (ok) await this._refresh();
  }

  _fileToB64(file) {
    return new Promise((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(String(r.result).split(',', 2)[1] || '');
      r.onerror = () => reject(new Error('read fail'));
      r.readAsDataURL(file);
    });
  }

  /* ================= data ================= */

  async _refresh() {
    // Loading skeleton (mục 20.4) — chỉ lần đầu hoặc khi list rỗng
    if (!this._assets.length) this._vlist?.setItems([]);
    const seq = ++this._searchSeq;
    const res = await call('asset_list', this._query, this._category, '', this._sort);
    if (seq !== this._searchSeq) return;         // stale — bỏ kết quả cũ (mục 18)
    if (!res?.ok) {
      handleError(res.error);
      this._renderGridError(res?.error);
      return;
    }
    this._assets = res.data.assets || [];
    this._vlist?.setItems(this._assets);
    this._renderPreview();                       // re-render selection state
  }

  _renderGridError(error) {
    // Error state với retry (mục 20.2/68 — error phải có recovery path)
    const mount = this.el.querySelector('#al-grid');
    if (!mount) return;
    mount.innerHTML = `<div class="empty">
      <div class="big">${icon('alert', 26)}</div>
      <div class="empty-title">${esc(t('al.loadError'))}</div>
      <div class="muted">${esc(error?.message || '')}</div>
      <button class="btn sm" id="al-retry" style="margin-top:10px">${icon('refresh', 13, 'btn-icon')}${esc(t('common.retry'))}</button>
    </div>`;
    mount.querySelector('#al-retry')?.addEventListener('click', () => this._refresh());
  }

  _card(asset, index) {
    const active = asset.id === this._selected;
    return `
      <div class="al-card${active ? ' al-active' : ''}" data-idx="${index}" data-id="${esc(asset.id)}"
           role="option" aria-selected="${active}" tabindex="0"
           title="${esc(asset.name)}">
        <img class="al-thumb al-skel" data-asset="${esc(asset.id)}" alt="" loading="lazy" />
        <div class="al-name truncate">${esc(asset.name)}</div>
        <div class="al-meta">${asset.width}×${asset.height} · ${esc(t('al.cat.' + asset.category))}</div>
      </div>`;
  }

  /**
   * Lazy thumbnail theo viewport (mục 18/20.4): IntersectionObserver load
   * khi card hiện ra — KHÔNG decode toàn bộ PNG cùng lúc. Called sau mỗi
   * lần vlist paint qua onRender hook.
   */
  _observeThumbs(root) {
    if (!this._thumbIO) {
      this._thumbIO = new IntersectionObserver((entries) => {
        for (const en of entries) {
          if (!en.isIntersecting) continue;
          const img = en.target;
          this._thumbIO.unobserve(img);
          this._loadThumb(img, img.dataset.asset);
        }
      }, { root: null, rootMargin: '80px' });
    }
    root.querySelectorAll('img.al-skel').forEach((img) => this._thumbIO.observe(img));
  }

  async _loadThumb(img, assetId) {
    // LRU cache (mục 79): key gồm hash — asset đổi nội dung tự miss
    const asset = this._assets.find((a) => a.id === assetId);
    const cacheKey = `${assetId}:${asset?.sha256?.slice(0, 12) || ''}`;
    const cached = thumbCache.get(cacheKey);
    if (cached) {
      img.src = cached;
      img.classList.remove('al-skel');
      return;
    }
    const res = await call('asset_get', assetId);
    if (!img.isConnected) return;
    if (res?.ok) {
      thumbCache.set(cacheKey, res.data.preview);
      img.src = res.data.preview;
      img.classList.remove('al-skel');
    } else {
      img.classList.remove('al-skel');
      img.classList.add('al-thumb-error');
    }
  }

  _onGridClick(e) {
    const card = e.target.closest('.al-card');
    if (!card) return;
    this._selected = card.dataset.id;
    this._refreshGridSelection();
    this._renderPreview();
  }

  _refreshGridSelection() {
    this.el.querySelectorAll('.al-card').forEach((c) => {
      const on = c.dataset.id === this._selected;
      c.classList.toggle('al-active', on);
      c.setAttribute('aria-selected', String(on));
    });
  }

  /* ================= preview panel (mục 50.3) ================= */

  _renderPreview() {
    const box = this.el.querySelector('#al-preview');
    if (!box) return;
    const asset = this._assets.find((a) => a.id === this._selected);
    if (!asset) {
      box.innerHTML = `<div class="muted">${esc(t('al.noSelection'))}</div>`;
      return;
    }
    box.innerHTML = `
      <h3>${icon('edit', 14)} ${esc(asset.name)}</h3>
      <div class="al-checker"><img id="al-big" alt="${esc(asset.name)}" /></div>
      <div class="al-meta-rows">
        <div class="row"><span class="muted">${esc(t('al.cat.label'))}</span><span>${esc(t('al.cat.' + asset.category))}</span></div>
        <div class="row"><span class="muted">${esc(t('al.dims'))}</span><span>${asset.width}×${asset.height}</span></div>
        <div class="row"><span class="muted">${esc(t('al.size'))}</span><span>${(asset.bytes / 1024).toFixed(1)} KB</span></div>
        <div class="row"><span class="muted">SHA256</span><span class="mono truncate" title="${esc(asset.sha256)}">${esc(asset.sha256.slice(0, 16))}…</span></div>
        ${asset.tags?.length ? `<div class="row"><span class="muted">${esc(t('al.tags'))}</span><span>${asset.tags.map((tg) => `<span class="badge">${esc(tg)}</span>`).join(' ')}</span></div>` : ''}
      </div>
      <div class="field"><label>${esc(t('al.assignLabel'))}</label>
        <select id="al-assign-project"></select>
        <input type="text" id="al-assign-path" value="assets/minecraft/textures/item/${esc(asset.name)}.png" />
        <button class="btn sm" id="al-assign">${icon('check', 12, 'btn-icon')}${esc(t('al.assign'))}</button>
      </div>
      <button class="btn sm danger block" id="al-delete">${icon('trash', 12, 'btn-icon')}${esc(t('common.delete'))}</button>`;

    this._loadThumb(box.querySelector('#al-big'), asset.id);

    // populate project select
    call('resource_list').then((res) => {
      if (!res?.ok) return;
      const sel = box.querySelector('#al-assign-project');
      if (!sel) return;
      sel.innerHTML = (res.data.projects || []).map((p) =>
        `<option value="${esc(p.id)}">${esc(p.name)} (${esc(p.minecraft?.version || '?')})</option>`).join('')
        || `<option value="">${esc(t('rs.none'))}</option>`;
    });

    box.querySelector('#al-assign')?.addEventListener('click', async () => {
      const pid = box.querySelector('#al-assign-project')?.value;
      const rel = box.querySelector('#al-assign-path')?.value?.trim();
      if (!pid) { toastError(t('rs.error.noProject')); return; }
      const res = await call('asset_assign', asset.id, pid, rel);
      if (!res?.ok) { handleError(res.error); return; }
      toastSuccess(t('al.toast.assigned', { path: res.data.path }));
    });

    box.querySelector('#al-delete')?.addEventListener('click', async () => {
      const ok = await confirmDialog({ title: t('al.deleteTitle'), message: t('al.deleteMsg', { name: asset.name }) });
      if (!ok) return;
      const res = await call('asset_delete', asset.id);
      if (!res?.ok) { handleError(res.error); return; }
      thumbCache.invalidate(`${asset.id}:${asset.sha256?.slice(0, 12) || ''}`);
      this._selected = null;
      toastSuccess(t('al.toast.deleted'));
      await this._refresh();
    });
  }
}
