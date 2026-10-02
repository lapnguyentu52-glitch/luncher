/**
 * HUD Editor subtab (spec 3.0 mục 11).
 *
 * - Canvas grid 854x480 (tỉ lệ GUI Minecraft) — drag&drop widget,
 *   snap grid 8px (mục 11: canvas/widget/resize/alignment/snap/grid).
 * - Toggle visible per widget + chọn project HUD để lưu layout
 *   (layout persist vào visual project qua backend — mục 34).
 * - Chỉ redraw khi state đổi (mục 79).
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

const GRID_W = 854, GRID_H = 480, SNAP = 8;

const WIDGET_COLORS = {
  fps: '#35e08f', cps: '#ffd75b', coordinates: '#5b9dff', armor: '#c8cdd8',
  potions: '#b06fe0', clock: '#5b9dff', ping: '#35e08f', server: '#c88a3c',
  biome: '#7ef29a', facing: '#c8cdd8', memory: '#ff8a5b', keystrokes: '#c8cdd8',
  target_info: '#ff4655', session_time: '#b06fe0',
};

export class HudStudioView {
  constructor(el) {
    this.el = el;
    this._layout = [];
    this._widgets = [];
    this._projectId = null;
    this._drag = null;   // {idx, dx, dy}
    this._unsubs = [];
  }

  async mount() {
    this.el.innerHTML = `
      <div class="vs-layout">
        <div class="card section vs-preview-card" style="margin-top:0">
          <h3>${icon('dashboard', 14)} ${t('vs.hud.canvas')}</h3>
          <div class="vs-canvas-wrap hud-wrap">
            <canvas id="hud-canvas" width="${GRID_W}" height="${GRID_H}"></canvas>
          </div>
          <div class="muted" style="margin-top:8px">${t('vs.hud.canvasHint')}</div>
        </div>

        <div class="card section" style="margin-top:0">
          <h3>${icon('edit', 14)} ${t('vs.hud.widgets')}</h3>
          <div id="hud-widget-list" class="hud-widget-list"></div>
          <div class="field" style="margin-top:12px"><label>${t('rs.open')}</label>
            <select id="hud-project"></select></div>
          <button class="btn primary block" id="hud-save">${icon('check', 15, 'btn-icon')}${t('vs.hud.save')}</button>
          <div class="muted" style="margin-top:8px">${t('vs.hud.saveHint')}</div>
        </div>
      </div>`;

    const [wRes, pRes] = await Promise.all([
      call('visual_hud_widgets'),
      call('resource_list'),
    ]);

    if (wRes?.ok) {
      this._widgets = wRes.data.widgets || [];
      if (!this._layout.length) this._layout = wRes.data.defaultLayout || [];
    }
    if (pRes?.ok) {
      const sel = this.el.querySelector('#hud-project');
      sel.innerHTML = (pRes.data.projects || []).map((p) =>
        `<option value="${esc(p.id)}">${esc(p.name)}</option>`).join('');
      sel.addEventListener('change', (e) => this._loadProject(e.target.value));
      if (pRes.data.projects?.length) {
        this._projectId = pRes.data.projects[0].id;
        await this._loadProject(this._projectId);
      }
    }

    this._renderWidgetList();
    this._draw();

    const canvas = this.el.querySelector('#hud-canvas');
    canvas.addEventListener('pointerdown', (e) => this._onDown(e));
    canvas.addEventListener('pointermove', (e) => this._onMove(e));
    canvas.addEventListener('pointerup', () => { this._drag = null; this._draw(); });
    canvas.addEventListener('pointerleave', () => { this._drag = null; });

    this.el.querySelector('#hud-save')?.addEventListener('click', () => this._save());

    this._unsubs.push(onBackendEvent('resource.built', () => {}));
  }

  async _loadProject(id) {
    if (!id) return;
    this._projectId = id;
    const res = await call('resource_get', id);
    if (!res?.ok) return;
    const hud = res.data.project?.visuals?.hud;
    if (Array.isArray(hud) && hud.length) {
      this._layout = hud;
      this._renderWidgetList();
      this._draw();
    }
  }

  _renderWidgetList() {
    const wrap = this.el.querySelector('#hud-widget-list');
    if (!wrap) return;
    const inLayout = new Map(this._layout.map((w, i) => [w.id, i]));
    wrap.innerHTML = this._widgets.map((wid) => {
      const idx = inLayout.get(wid);
      const on = idx !== undefined && this._layout[idx].visible;
      return `
        <label class="row" style="cursor:pointer">
          <input type="checkbox" data-wid="${esc(wid)}" ${on ? 'checked' : ''}
                 style="width:auto;flex:0 0 auto;margin-right:8px" />
          <span class="hud-dot" style="background:${WIDGET_COLORS[wid] || '#888'}"></span>
          <span class="grow">${esc(t('vs.hud.widget.' + wid))}</span>
        </label>`;
    }).join('');

    wrap.querySelectorAll('input[type="checkbox"]').forEach((cb) => {
      cb.addEventListener('change', () => {
        const wid = cb.dataset.wid;
        const idx = this._layout.findIndex((w) => w.id === wid);
        if (cb.checked) {
          if (idx < 0) this._layout.push({ id: wid, x: 8, y: 8, scale: 1, visible: true });
          else this._layout[idx].visible = true;
        } else if (idx >= 0) this._layout[idx].visible = false;
        this._draw();
      });
    });
  }

  /* ---------------- Canvas: grid + widgets + drag (mục 11) ---------------- */

  _widgetAt(px, py) {
    for (let i = this._layout.length - 1; i >= 0; i--) {
      const w = this._layout[i];
      if (!w.visible) continue;
      const bw = 70 * w.scale, bh = 20 * w.scale;
      if (px >= w.x && px <= w.x + bw && py >= w.y && py <= w.y + bh) return i;
    }
    return -1;
  }

  _onDown(e) {
    const rect = e.target.getBoundingClientRect();
    const px = (e.clientX - rect.left) * (GRID_W / rect.width);
    const py = (e.clientY - rect.top) * (GRID_H / rect.height);
    const idx = this._widgetAt(px, py);
    if (idx >= 0) {
      const w = this._layout[idx];
      this._drag = { idx, dx: px - w.x, dy: py - w.y };
      e.target.setPointerCapture(e.pointerId);
    }
  }

  _onMove(e) {
    if (!this._drag) return;
    const rect = e.target.getBoundingClientRect();
    const px = (e.clientX - rect.left) * (GRID_W / rect.width);
    const py = (e.clientY - rect.top) * (GRID_H / rect.height);
    const w = this._layout[this._drag.idx];
    // snap grid 8px + clamp trong canvas (mục 11)
    w.x = Math.max(0, Math.min(GRID_W - 70 * w.scale, Math.round((px - this._drag.dx) / SNAP) * SNAP));
    w.y = Math.max(0, Math.min(GRID_H - 20 * w.scale, Math.round((py - this._drag.dy) / SNAP) * SNAP));
    this._draw();
  }

  _draw() {
    const canvas = this.el.querySelector('#hud-canvas');
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const css = getComputedStyle(document.documentElement);

    ctx.fillStyle = css.getPropertyValue('--bg-1').trim() || '#0d0f14';
    ctx.fillRect(0, 0, GRID_W, GRID_H);

    // grid 8px mờ
    ctx.strokeStyle = css.getPropertyValue('--border-0').trim() || 'rgba(255,255,255,.05)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let x = 0; x <= GRID_W; x += 64) { ctx.moveTo(x, 0); ctx.lineTo(x, GRID_H); }
    for (let y = 0; y <= GRID_H; y += 64) { ctx.moveTo(0, y); ctx.lineTo(GRID_W, y); }
    ctx.stroke();

    // widgets visible
    ctx.textBaseline = 'top';
    for (const w of this._layout) {
      if (!w.visible) continue;
      const bw = 70 * w.scale, bh = 20 * w.scale;
      ctx.fillStyle = 'rgba(10,12,18,.72)';
      ctx.fillRect(w.x, w.y, bw, bh);
      ctx.strokeStyle = WIDGET_COLORS[w.id] || '#888';
      ctx.lineWidth = 1.5;
      ctx.strokeRect(w.x + 0.5, w.y + 0.5, bw - 1, bh - 1);
      ctx.fillStyle = WIDGET_COLORS[w.id] || '#ccc';
      ctx.font = `${11 * w.scale}px sans-serif`;
      ctx.fillText(t('vs.hud.widget.' + w.id), w.x + 6, w.y + bh / 2 - 6 * w.scale);
    }
  }

  async _save() {
    if (!this._projectId) { toastError(t('rs.error.noProject')); return; }
    const res = await call('visual_save_hud_layout', this._projectId, this._layout);
    if (!res?.ok) { handleError(res.error); return; }
    this._layout = res.data.layout;
    this._draw();
    toastSuccess(t('vs.hud.toast.saved'));
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
  }
}
