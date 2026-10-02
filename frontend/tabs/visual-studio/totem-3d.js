/**
 * Totem 3D Studio (master plan v3 mục 10, 54–62 — Batch 4).
 *
 * Workspace: outliner (cubes) | viewport | inspector (transform/materials).
 * Viewport: canvas 2.5D dùng CÙNG projection isometric với backend static
 * renderer (services/visuals/renderer.py) — preview khớp fallback PNG, không
 * cần GPU/three.js (offline-first mục 38; WebGL true 3D là lazy upgrade sau,
 * fallback-first đúng mục 9/19: WebGL không khả dụng vẫn edit được).
 *
 * Lifecycle (mục 11): mount tạo mọi thứ; unmount hủy listener + rAF + timer,
 * autosave flush; KHÔNG giữ DOM ref ngoài this.el; render loop chỉ chạy khi
 * dirty (on-demand, không rAF vô hạn — mục 19.2 không render khi tab ẩn).
 *
 * Undo/redo (mục 61): command pattern đơn giản — push snapshot spec; merge
 * drag liên tục theo debounce; autosave 600ms debounce, atomic (backend).
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { confirmDialog } from '../../app/modal.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const FACES = ['up', 'east', 'south'];
const SHADE = { up: 1.0, east: 0.8, south: 0.62 };
const HEX_RE = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;
const AUTOSAVE_MS = 600;
const MAX_CUBES_SOFT = 64;      // mục 35: 32 khuyến nghị, 64 warning

const DEFAULT_SPEC = {
  grid: 16,
  cubes: [
    { id: 'body', from: [4, 0, 4], to: [12, 10, 12], faces: { north: { texture: 'base' } } },
    { id: 'head', from: [4, 10, 4], to: [12, 16, 12], faces: { north: { texture: 'eye' } } },
  ],
  textures: { base: '#e8b23a', accent: '#8c5a2b', eye: '#3ad0e8' },
};

function hexRgb(color) {
  let c = String(color || '').replace('#', '');
  if (c.length === 3) c = c.split('').map((x) => x + x).join('');
  if (!/^[0-9a-fA-F]{6}$/.test(c)) return [232, 178, 58];
  return [parseInt(c.slice(0, 2), 16), parseInt(c.slice(2, 4), 16), parseInt(c.slice(4, 6), 16)];
}
const clampInt = (v, lo, hi) => Math.max(lo, Math.min(hi, Math.round(Number(v) || 0)));

export class Totem3DView {
  constructor(el) {
    this.el = el;
    this._spec = null;             // spec hiện tại (draft)
    this._selected = null;         // cube id
    this._undo = [];
    this._redo = [];
    this._dirty = false;
    this._raf = 0;
    this._saveTimer = 0;
    this._unsubs = [];
    this._cam = 0;                 // yaw 0..3 (quay 90°) — deterministic orbit
  }

  /* ================= lifecycle ================= */

  async mount() {
    this.el.innerHTML = `
      <div class="t3-wrap">
        <div class="t3-toolbar" role="toolbar" aria-label="${esc(t('t3.title'))}">
          <button class="btn sm" id="t3-add" title="${esc(t('t3.addCube'))}">${icon('plus', 13, 'btn-icon')}${esc(t('t3.addCube'))}</button>
          <button class="btn sm" id="t3-dup" title="${esc(t('t3.duplicate'))}">${icon('copy', 13, 'btn-icon')}${esc(t('t3.duplicate'))}</button>
          <button class="btn sm danger" id="t3-del" title="${esc(t('t3.delete'))}">${icon('trash', 13, 'btn-icon')}${esc(t('t3.delete'))}</button>
          <span class="t3-sep"></span>
          <button class="icon-btn plain" id="t3-undo" title="${esc(t('t3.undo'))} (Ctrl+Z)">${icon('arrowLeft', 15)}</button>
          <button class="icon-btn plain" id="t3-redo" title="${esc(t('t3.redo'))} (Ctrl+Y)">${icon('arrowRight', 15)}</button>
          <span class="t3-sep"></span>
          <button class="icon-btn plain" id="t3-rot" title="${esc(t('t3.rotate'))} (R)">${icon('refresh', 15)}</button>
          <button class="btn sm" id="t3-quick">${icon('sparkle', 13, 'btn-icon')}${esc(t('t3.fromQuick'))}</button>
          <span class="grow"></span>
          <span id="t3-saved" class="muted" aria-live="polite"></span>
        </div>
        <div class="t3-cols">
          <div class="card section t3-outliner">
            <h3>${icon('layers', 14)} ${esc(t('t3.objects'))}</h3>
            <div id="t3-list" class="list" role="listbox" aria-label="${esc(t('t3.objects'))}"></div>
          </div>
          <div class="t3-viewport-col">
            <div class="card t3-viewport">
              <canvas id="t3-canvas" width="520" height="420"
                      aria-label="${esc(t('t3.viewport'))}" tabindex="0"></canvas>
              <img id="t3-fallback" alt="${esc(t('t3.viewport'))}" hidden />
            </div>
            <div class="muted t3-hint">${esc(t('t3.viewportHint'))}</div>
          </div>
          <div class="card section t3-inspector">
            <h3>${icon('edit', 14)} ${esc(t('t3.inspector'))}</h3>
            <div id="t3-insp"></div>
          </div>
        </div>
      </div>`;

    this._bindToolbar();
    this._bindCanvas();
    this._bindKeys();

    const res = await call('visual_totem_model_get');
    const spec = res?.ok ? res.data.spec : null;
    this._spec = spec || JSON.parse(JSON.stringify(DEFAULT_SPEC));
    if (spec && Array.isArray(res.data.findings) && res.data.findings.length) {
      this._showFindings(res.data.findings);
    }
    this._saved();
    this._refreshAll();
  }

  unmount() {
    // mục 11: hủy rAF, timer, listeners — không giữ gì sống sót sau unmount
    if (this._raf) { cancelAnimationFrame(this._raf); this._raf = 0; }
    // Batch 8 fix: flush draft TRƯỚC khi clear timer — trước đây timer bị
    // clear rồi flush bị guard chặn -> mất draft nếu rời tab <600ms (mục 62)
    if (this._dirty) this._flushSave();
    if (this._saveTimer) { clearTimeout(this._saveTimer); this._saveTimer = 0; }
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._spec = null;
    this._selected = null;
    this._undo = [];
    this._redo = [];
  }

  /* ================= toolbar / events ================= */

  _bindToolbar() {
    const on = (id, fn) => this.el.querySelector(id)?.addEventListener('click', fn);
    on('#t3-add', () => this._addCube());
    on('#t3-dup', () => this._dupCube());
    on('#t3-del', () => this._delCube());
    on('#t3-undo', () => this._undoOp());
    on('#t3-redo', () => this._redoOp());
    on('#t3-rot', () => { this._cam = (this._cam + 1) % 4; this._render(); });
    on('#t3-quick', () => this._fromQuick());
  }

  _bindCanvas() {
    const cv = this.el.querySelector('#t3-canvas');
    if (!cv) return;
    // Không WebGL cần thiết — canvas 2D deterministic, mất context = vẫn chạy
    cv.addEventListener('click', (e) => this._pick(e));
    cv.addEventListener('wheel', (e) => {
      e.preventDefault();
      this._zoom(clampInt(e.deltaY > 0 ? -1 : 1, -1, 1));
    }, { passive: false });
  }

  _bindKeys() {
    const onKey = (e) => {
      // mục 56: không override khi input focus
      const tag = document.activeElement?.tagName;
      if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
      if (e.key === 'Delete' && this._selected) { e.preventDefault(); this._delCube(); }
      else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') { e.preventDefault(); this._undoOp(); }
      else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'y') { e.preventDefault(); this._redoOp(); }
      else if (e.key.toLowerCase() === 'r' && !e.ctrlKey && !e.metaKey) { this._cam = (this._cam + 1) % 4; this._render(); }
      else if (e.key === 'Escape') { this._selected = null; this._refreshAll(); }
    };
    document.addEventListener('keydown', onKey);
    this._unsubs.push(() => document.removeEventListener('keydown', onKey));
  }

  /* ================= spec ops (undo/redo qua snapshot — mục 61) ================= */

  _pushHistory() {
    this._undo.push(JSON.stringify(this._spec));
    if (this._undo.length > 100) this._undo.shift();
    this._redo = [];
    this._markDirty();
  }

  _undoOp() {
    if (!this._undo.length) return;
    this._redo.push(JSON.stringify(this._spec));
    this._spec = JSON.parse(this._undo.pop());
    this._selected = this._spec.cubes.some((c) => c.id === this._selected) ? this._selected : null;
    this._markDirty();
    this._refreshAll();
  }

  _redoOp() {
    if (!this._redo.length) return;
    this._undo.push(JSON.stringify(this._spec));
    this._spec = JSON.parse(this._redo.pop());
    this._markDirty();
    this._refreshAll();
  }

  _markDirty() {
    this._dirty = true;
    const el = this.el.querySelector('#t3-saved');
    if (el) el.textContent = t('t3.unsaved');
    if (this._saveTimer) clearTimeout(this._saveTimer);
    this._saveTimer = setTimeout(() => this._flushSave(), AUTOSAVE_MS);
  }

  async _flushSave() {
    if (this._saveTimer) { clearTimeout(this._saveTimer); this._saveTimer = 0; }
    if (!this._dirty || !this._spec) return;
    this._dirty = false;
    const res = await call('visual_totem_model_save', this._spec);
    if (!res?.ok) { handleError(res.error); this._dirty = true; return; }
    this._saved();
  }

  _saved() {
    const el = this.el.querySelector('#t3-saved');
    if (el) el.textContent = t('t3.saved');
  }

  _mutate(fn) {
    this._pushHistory();
    fn(this._spec);
    this._refreshAll();
  }

  /* ================= cube ops (mục 8.2) ================= */

  _addCube() {
    if (this._spec.cubes.length >= MAX_CUBES_SOFT) { toastError(t('t3.limit')); return; }
    this._mutate((s) => {
      const id = this._uniqueId('cube');
      const n = s.cubes.length;
      s.cubes.push({ id, from: [n * 2 % 16, 0, 0], to: [n * 2 % 16 + 2, 2, 2],
                     faces: {} });
      this._selected = id;
    });
  }

  _dupCube() {
    const src = this._spec.cubes.find((c) => c.id === this._selected);
    if (!src) return;
    if (this._spec.cubes.length >= MAX_CUBES_SOFT) { toastError(t('t3.limit')); return; }
    this._mutate((s) => {
      const id = this._uniqueId(src.id);
      const copy = JSON.parse(JSON.stringify(src));
      copy.id = id;
      copy.from = copy.from.map((v) => v + 1);
      copy.to = copy.to.map((v) => v + 1);
      s.cubes.push(copy);
      this._selected = id;
    });
  }

  async _delCube() {
    const cube = this._spec.cubes.find((c) => c.id === this._selected);
    if (!cube) return;
    const ok = await confirmDialog({ title: t('t3.deleteTitle'), message: t('t3.deleteMsg', { id: cube.id }) });
    if (!ok) return;
    this._mutate((s) => {
      s.cubes = s.cubes.filter((c) => c.id !== this._selected);
      this._selected = null;
    });
  }

  _uniqueId(base) {
    let n = 2;
    let id = base;
    while (this._spec.cubes.some((c) => c.id === id)) id = `${base}-${n++}`;
    return id;
  }

  async _fromQuick() {
    // mục 70: Quick preset -> default voxel model; màu lấy từ handoff nếu có
    // (Batch 8: bỏ call render vô nghĩa — màu truyền qua sessionStorage)
    let colors = null;
    try {
      const raw = sessionStorage.getItem('antares.totem3d.colors');
      if (raw) colors = JSON.parse(raw);
    } catch { /* no-op */ }
    const textures = {
      base: colors?.base || '#e8b23a',
      accent: colors?.accent || '#8c5a2b',
      eye: colors?.eye || '#3ad0e8',
    };
    this._mutate((s) => {
      s.grid = 16;
      s.textures = textures;
      s.cubes = [
        { id: 'body', from: [4, 0, 4], to: [12, 10, 12],
          faces: { north: { texture: 'base' }, south: { texture: 'base' },
                   east: { texture: 'base' }, west: { texture: 'base' },
                   up: { texture: 'base' }, down: { texture: 'base' } } },
        { id: 'head', from: [4, 10, 4], to: [12, 16, 12],
          faces: { north: { texture: 'eye' }, south: { texture: 'accent' },
                   east: { texture: 'accent' }, west: { texture: 'accent' },
                   up: { texture: 'accent' }, down: { texture: 'accent' } } },
      ];
      this._selected = null;
    });
    try { sessionStorage.removeItem('antares.totem3d.colors'); } catch { /* no-op */ }
    toastSuccess(t('t3.fromQuickDone'));
  }

  /* ================= outliner ================= */

  _refreshOutliner() {
    const wrap = this.el.querySelector('#t3-list');
    if (!wrap) return;
    wrap.innerHTML = this._spec.cubes.map((c, i) => `
      <div class="row t3-cube${c.id === this._selected ? ' t3-active' : ''}" data-id="${esc(c.id)}" role="option"
           aria-selected="${c.id === this._selected}" tabindex="0">
        <span class="t3-eye" data-eye="${esc(c.id)}" title="${esc(t('t3.hide'))}">${icon('eye', 13)}</span>
        <span class="grow">${esc(c.id)}</span>
        <span class="muted">${c.to.map((v, k) => Math.abs(v - c.from[k])).join('×')}</span>
      </div>`).join('')
      || `<div class="muted" style="padding:8px 0">${esc(t('t3.noCubes'))}</div>`;
    if (this._spec.cubes.length > MAX_CUBES_SOFT) {
      wrap.insertAdjacentHTML('beforeend',
        `<div class="muted">${esc(t('t3.limitWarn'))}</div>`);
    }
    wrap.querySelectorAll('.t3-cube').forEach((row) => {
      row.addEventListener('click', (e) => {
        if (e.target.closest('[data-eye]')) return;
        this._selected = row.dataset.id;
        this._refreshAll();
      });
      row.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); this._selected = row.dataset.id; this._refreshAll(); }
      });
    });
  }

  /* ================= inspector ================= */

  _refreshInspector() {
    const wrap = this.el.querySelector('#t3-insp');
    if (!wrap) return;
    const cube = this._spec.cubes.find((c) => c.id === this._selected);
    if (!cube) {
      wrap.innerHTML = `<div class="muted">${esc(t('t3.noSelection'))}</div>`;
      return;
    }
    const axes = ['x', 'y', 'z'];
    const numRow = (label, key, idx) => `
      <div class="t3-field"><label>${label}${axes[idx]}</label>
        <input type="number" step="1" data-num="${key}.${idx}" value="${cube[key][idx]}" /></div>`;
    wrap.innerHTML = `
      <div class="field"><label>${esc(t('t3.cubeId'))}</label>
        <input type="text" data-text="id" value="${esc(cube.id)}" /></div>
      <div class="t3-grid">
        ${[0, 1, 2].map((i) => numRow('From ', 'from', i)).join('')}
        ${[0, 1, 2].map((i) => numRow('To ', 'to', i)).join('')}
      </div>
      <div class="field"><label>${esc(t('t3.materialNorth'))}</label>
        <input type="color" data-face="north" value="${this._faceColor(cube, 'north')}" /></div>
      <div class="field"><label>${esc(t('t3.materialAsset'))}</label>
        <select id="t3-asset-pick"><option value="">${esc(t('t3.materialAssetNone'))}</option></select>
        <button class="btn sm" id="t3-asset-apply">${icon('check', 12, 'btn-icon')}${esc(t('al.assign'))}</button>
      </div>
      <div class="muted">${esc(t('t3.materialHint'))}</div>`;

    // Material picker từ Asset Library (Batch 5 — mục 60: Project Assets)
    call('asset_list', '', '', '', 'name').then((res) => {
      const sel = wrap.querySelector('#t3-asset-pick');
      if (!sel || !res?.ok) return;
      const opts = (res.data.assets || []).map((a) =>
        `<option value="assets/minecraft/textures/${esc(a.category)}/${esc(a.name)}.png">${esc(a.name)} (${a.width}×${a.height})</option>`);
      if (opts.length) sel.insertAdjacentHTML('beforeend', opts.join(''));
    });
    wrap.querySelector('#t3-asset-apply')?.addEventListener('click', () => {
      const rel = wrap.querySelector('#t3-asset-pick')?.value;
      if (!rel) return;
      this._mutate((s) => {
        const c = s.cubes.find((x) => x.id === cube.id);
        if (!c) return;
        c.faces = c.faces || {};
        c.faces.north = { texture: rel };       // asset path -> compile elements
      });
    });

    wrap.querySelectorAll('[data-num]').forEach((inp) => {
      inp.addEventListener('change', () => {
        const [key, idx] = inp.dataset.num.split('.');
        this._mutate((s) => {
          const c = s.cubes.find((x) => x.id === cube.id);
          if (c) c[key][+idx] = clampInt(inp.value, -16, 32);
        });
      });
    });
    wrap.querySelector('[data-text]')?.addEventListener('change', (e) => {
      const val = e.target.value.trim();
      if (!val) return;
      this._mutate((s) => {
        const c = s.cubes.find((x) => x.id === cube.id);
        if (!c) return;
        const nid = this._uniqueId(val);
        c.id = nid;
        this._selected = nid;
      });
    });
    wrap.querySelector('[data-face]')?.addEventListener('input', (e) => {
      const color = e.target.value;
      this._mutate((s) => {
        const c = s.cubes.find((x) => x.id === cube.id);
        if (!c) return;
        c.faces = c.faces || {};
        c.faces.north = { texture: color };
        s.textures = s.textures || {};
      });
    });
  }

  _faceColor(cube, face) {
    const f = cube.faces?.[face];
    const v = typeof f === 'object' ? f?.texture : f;
    if (typeof v === 'string' && HEX_RE.test(v)) return v;
    const key = typeof v === 'string' ? this._spec.textures?.[v] : null;
    return (typeof key === 'string' && HEX_RE.test(key)) ? key : '#e8b23a';
  }

  /* ================= viewport (2.5D — cùng projection backend renderer) ================= */

  _pick(e) {
    const cv = this.el.querySelector('#t3-canvas');
    if (!cv) return;
    const rect = cv.getBoundingClientRect();
    const x = (e.clientX - rect.left) * (cv.width / rect.width);
    const y = (e.clientY - rect.top) * (cv.height / rect.height);
    const hit = this._hitTest(x, y);
    this._selected = hit ? hit.id : null;
    this._refreshAll();
  }

  _hitTest(px, py) {
    // Dùng face list đã render — chọn face có depth lớn nhất chứa điểm click
    const cv = this.el.querySelector('#t3-canvas');
    if (!cv) return null;
    const ctx = cv.getContext('2d');
    const data = ctx.getImageData(px, py, 1, 1).data;
    if (data[3] === 0) return null;
    // hit theo outliner geometry: duyệt faces vẽ sau (depth cao) trước
    const faces = this._collectFaces(cv.width, cv.height);
    let best = null;
    for (const f of faces) {
      if (px >= f.bx[0] && px <= f.bx[1] && py >= f.by[0] && py <= f.by[1]) {
        if (!best || f.depth > best.depth) best = f;
      }
    }
    return best;
  }

  _collectFaces(W, H) {
    // Geometry 2.5D: sx = x - z (xoay theo _cam), sy = (x + z) * 0.5 - y
    const faces = [];
    const yaw = this._cam;
    const rot = (x, y, z) => {
      for (let i = 0; i < yaw; i++) { const nx = z, nz = 15 - x; x = nx; z = nz; }
      return [x, y, z];
    };
    const proj = (p) => [p[0] - p[2], (p[0] + p[2]) * 0.5 - p[1]];
    for (const cube of this._spec.cubes) {
      const id = cube.id;
      const lo = [Math.min(cube.from[0], cube.to[0]), Math.min(cube.from[1], cube.to[1]), Math.min(cube.from[2], cube.to[2])];
      const hi = [Math.max(cube.from[0], cube.to[0]), Math.max(cube.from[1], cube.to[1]), Math.max(cube.from[2], cube.to[2])];
      const dx = hi[0] - lo[0], dy = hi[1] - lo[1], dz = hi[2] - lo[2];
      if (dx <= 0 || dy <= 0 || dz <= 0) continue;
      const [x0, y0, z0] = rot(lo[0], lo[1], lo[2]);
      const visible = yaw % 2 === 0
        ? [['south', [dx, 0, 0], [0, dy, 0], lo, hi], ['east', [0, 0, dz], [0, dy, 0], lo, hi]]
        : [['north', [dx, 0, 0], [0, dy, 0], lo, hi], ['west', [0, 0, dz], [0, dy, 0], lo, hi]];
      visible.push(['up', [dx, 0, 0], [0, 0, dz], lo, hi]);
      for (const [face, e1, e2, l, h] of visible) {
        const origin = face === 'up' ? [x0, h[1], z0]
          : face === 'south' || face === 'north' ? [x0, y0, face === 'south' ? h[2] : z0]
            : [face === 'east' ? h[0] : x0, y0, z0];
        const p0 = proj(rot(origin[0], origin[1], origin[2]));
        const pa = proj(rot(origin[0] + e1[0], origin[1] + e1[1], origin[2] + e1[2]));
        const pb = proj(rot(origin[0] + e2[0], origin[1] + e2[1], origin[2] + e2[2]));
        const depth = origin[0] + origin[1] + origin[2]
          + (e1[0] + e1[1] + e1[2]) * 0.5 + (e2[0] + e2[1] + e2[2]) * 0.5;
        faces.push({ id, face, p0, a: [pa[0] - p0[0], pa[1] - p0[1]],
                     b: [pb[0] - p0[0], pb[1] - p0[1]], depth });
      }
    }
    void W; void H;
    return faces.sort((f1, f2) => f1.depth - f2.depth);
  }

  _zoom(_dir) { /* fit auto theo bounds — reserved cho zoom factor sau */ }

  _render() {
    const cv = this.el.querySelector('#t3-canvas');
    if (!cv) return;
    const ctx = cv.getContext('2d');
    const W = cv.width, H = cv.height;
    ctx.clearRect(0, 0, W, H);
    // Empty state overlay (mục 20.3) — vẽ ngay, không đợi rAF
    if (!this._spec?.cubes?.length) {
      this._drawEmpty(ctx, W, H);
      return;
    }
    if (this._raf) { cancelAnimationFrame(this._raf); this._raf = 0; }
    this._raf = requestAnimationFrame(() => {
      this._raf = 0;
      if (!this.el.isConnected || !this._spec) return;    // tab ẩn/unmounted
      const faces = this._collectFaces(W, H);
      if (!faces.length) return;
      // bounds fit
      let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
      for (const f of faces) {
        for (const [ex, ey] of [[0, 0], [1, 0], [1, 1], [0, 1]]) {
          const x = f.p0[0] + ex * f.a[0] + ey * f.b[0];
          const y = f.p0[1] + ex * f.a[1] + ey * f.b[1];
          minX = Math.min(minX, x); maxX = Math.max(maxX, x);
          minY = Math.min(minY, y); maxY = Math.max(maxY, y);
        }
      }
      const span = Math.max(maxX - minX, maxY - minY) || 1;
      const pad = 24;
      const scale = (W - pad * 2) / span;
      const offX = (W - (maxX - minX) * scale) / 2 - minX * scale;
      const offY = (H - (maxY - minY) * scale) / 2 - minY * scale;
      const px = (x, y) => [x * scale + offX, y * scale + offY];

      ctx.imageSmoothingEnabled = false;
      for (const f of faces) {   // depth tăng dần -> painter back-to-front
        const cube = this._spec.cubes.find((c) => c.id === f.id);
        const colorKey = typeof (cube?.faces?.[f.face]) === 'object'
          ? cube?.faces?.[f.face]?.texture : cube?.faces?.[f.face];
        const rgb = hexRgb(typeof colorKey === 'string' && HEX_RE.test(colorKey)
          ? colorKey : (this._spec.textures?.[colorKey] || '#e8b23a'));
        const sh = SHADE[f.face] ?? 0.7;
        const [x0, y0] = px(f.p0[0], f.p0[1]);
        const [xa, ya] = px(f.p0[0] + f.a[0], f.p0[1] + f.a[1]);
        const [xb, yb] = px(f.p0[0] + f.b[0], f.p0[1] + f.b[1]);
        ctx.beginPath();
        ctx.moveTo(x0, y0);
        ctx.lineTo(xa, ya);
        ctx.lineTo(xa + xb - x0, ya + yb - y0);
        ctx.lineTo(xb, yb);
        ctx.closePath();
        ctx.fillStyle = `rgb(${Math.round(rgb[0] * sh)},${Math.round(rgb[1] * sh)},${Math.round(rgb[2] * sh)})`;
        ctx.fill();
        ctx.strokeStyle = 'rgba(0,0,0,.35)';
        ctx.stroke();
        if (f.id === this._selected) {
          ctx.strokeStyle = 'rgba(255,92,71,.95)';
          ctx.lineWidth = 2;
          ctx.stroke();
          ctx.lineWidth = 1;
        }
      }
      // hit-test bounds cache
      for (const f of faces) {
        const [x0, y0] = px(f.p0[0], f.p0[1]);
        const [xa, ya] = px(f.p0[0] + f.a[0], f.p0[1] + f.a[1]);
        const [xb, yb] = px(f.p0[0] + f.b[0], f.p0[1] + f.b[1]);
        f.bx = [Math.min(x0, xa, xa + xb - x0, xb), Math.max(x0, xa, xa + xb - x0, xb)];
        f.by = [Math.min(y0, ya, ya + yb - y0, yb), Math.max(y0, ya, ya + yb - y0, yb)];
      }
      this._faces = faces;
    });
  }

  _drawEmpty(ctx, W, H) {
    ctx.save();
    ctx.fillStyle = 'rgba(133, 141, 157, .9)';   // --text-2
    ctx.font = '13px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText(t('t3.noCubes'), W / 2, H / 2);
    ctx.restore();
  }

  /* ================= misc ================= */

  _showFindings(findings) {
    const bad = findings.filter((f) => f.severity === 'ERROR' || f.severity === 'FATAL');
    if (bad.length) toastError(`${bad.length} ${t('t3.invalidModel')}`);
  }

  _refreshAll() {
    this._refreshOutliner();
    this._refreshInspector();
    this._render();
  }
}
