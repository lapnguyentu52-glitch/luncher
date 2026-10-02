/**
 * Visual Studio — Crosshair Studio (spec 3.0 mục 8).
 *
 * - Canvas preview vẽ cùng hình học với renderer backend (mục 8.2: local
 *   preview, không load web ngoài) — mỗi thay đổi redraw 1 lần, KHÔNG rAF
 *   loop khi state tĩnh (mục 79). Animated (dynamic spread) là giai đoạn sau.
 * - Controls: shape/thickness/gap/color/outline/dot/opacity + presets (8.1).
 * - Export: tạo resource pack qua pipeline Resource Studio (output mode 8.3)
 *   + cài thẳng vào instance tuỳ chọn.
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { openModal } from '../../app/modal.js';
import { onBackendEvent } from '../../app/events.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const DEFAULT_SPEC = {
  shape: 'cross', size: 16, thickness: 1, gap: 3,
  color: '#ff4655', outline: true, outlineColor: '#000000',
  dot: false, opacity: 0.9,
};

const PRESET_IDS = ['minimal', 'dot', 'classic', 'thin', 'pvp', 'clean', 'competitive'];

export class CrosshairStudioView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._spec = { ...DEFAULT_SPEC };
    this._presetId = null;
    this._previewDirty = false;
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('vs.title')}</div>
          <div class="view-sub">${t('vs.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn primary" id="vs-export">${icon('check', 15, 'btn-icon')}${t('vs.export')}</button>
        </div>
      </div>

      <div class="vs-layout">
        <div class="card section vs-preview-card" style="margin-top:0">
          <h3>${icon('rocket', 14)} ${t('vs.preview')}</h3>
          <div class="vs-canvas-wrap">
            <canvas id="vs-canvas" width="320" height="320" aria-label="${esc(t('vs.preview'))}"></canvas>
          </div>
          <div class="muted" style="margin-top:8px">${t('vs.previewHint')}</div>
          <div class="vs-real" id="vs-real">
            <img id="vs-real-img" alt="" width="16" height="16" />
            <span class="muted">${t('vs.renderHint')}</span>
          </div>
        </div>

        <div class="card section" style="margin-top:0">
          <h3>${icon('edit', 14)} ${t('vs.controls')}</h3>
          <div class="field"><label>${t('vs.preset')}</label>
            <div id="vs-presets" class="opt-profiles"></div></div>
          <div class="field"><label>${t('vs.shape')}</label>
            <select id="vs-shape">
              <option value="cross">${t('vs.shape.cross')}</option>
              <option value="dot">${t('vs.shape.dotOnly')}</option>
              <option value="circle">${t('vs.shape.circle')}</option>
            </select></div>
          <div class="field"><label>${t('vs.thickness')}: <span id="vs-thickness-v"></span></label>
            <input type="range" id="vs-thickness" min="1" max="4" step="1" /></div>
          <div class="field"><label>${t('vs.gap')}: <span id="vs-gap-v"></span></label>
            <input type="range" id="vs-gap" min="0" max="7" step="1" /></div>
          <div class="field"><label>${t('vs.color')}</label>
            <input type="color" id="vs-color" /></div>
          <div class="field">
            <label class="switch"><input type="checkbox" id="vs-outline" />
              <span class="track"></span>${t('vs.outline')}</label></div>
          <div class="field">
            <label class="switch"><input type="checkbox" id="vs-dot" />
              <span class="track"></span>${t('vs.dotCenter')}</label></div>
          <div class="field"><label>${t('vs.opacity')}: <span id="vs-opacity-v"></span></label>
            <input type="range" id="vs-opacity" min="0.1" max="1" step="0.05" /></div>
        </div>
      </div>`;

    this._bindControls();
    this._renderPresets();
    this._drawPreview();
    this._scheduleRealPreview();

    this._unsubs.push(onBackendEvent('visual.exported', () => {
      toastSuccess(t('vs.toast.exported'));
    }));
  }

  /* ---------------- Controls ---------------- */

  _bindControls() {
    const $ = (id) => this.el.querySelector(id);
    $('vs-shape').addEventListener('change', (e) => { this._spec.shape = e.target.value; this._changed(); });
    $('vs-thickness').addEventListener('input', (e) => {
      this._spec.thickness = Number(e.target.value); this._changed();
    });
    $('vs-gap').addEventListener('input', (e) => { this._spec.gap = Number(e.target.value); this._changed(); });
    $('vs-color').addEventListener('input', (e) => { this._spec.color = e.target.value; this._changed(); });
    $('vs-outline').addEventListener('change', (e) => { this._spec.outline = e.target.checked; this._changed(); });
    $('vs-dot').addEventListener('change', (e) => { this._spec.dot = e.target.checked; this._changed(); });
    $('vs-opacity').addEventListener('input', (e) => {
      this._spec.opacity = Number(e.target.value); this._changed();
    });
    $('vs-export').addEventListener('click', () => this._export());

    this._syncControls();
  }

  _syncControls() {
    const $ = (id) => this.el.querySelector(id);
    $('vs-shape').value = this._spec.shape;
    $('vs-thickness').value = this._spec.thickness;
    $('vs-thickness-v').textContent = this._spec.thickness;
    $('vs-gap').value = this._spec.gap;
    $('vs-gap-v').textContent = this._spec.gap;
    $('vs-color').value = this._spec.color;
    $('vs-outline').checked = this._spec.outline;
    $('vs-dot').checked = this._spec.dot;
    $('vs-opacity').value = this._spec.opacity;
    $('vs-opacity-v').textContent = Math.round(this._spec.opacity * 100) + '%';
  }

  _renderPresets() {
    const wrap = this.el.querySelector('#vs-presets');
    if (!wrap) return;
    wrap.innerHTML = PRESET_IDS.map((id) => `
      <button class="chip${id === this._presetId ? ' active' : ''}" data-preset="${id}">
        ${esc(t('vs.preset.' + id))}
      </button>`).join('');
    wrap.querySelectorAll('[data-preset]').forEach((btn) => {
      btn.addEventListener('click', async () => {
        this._presetId = btn.dataset.preset;
        const res = await call('visual_presets');
        if (res?.ok) {
          const p = (res.data.presets || []).find((x) => x.id === this._presetId);
          if (p) {
            this._spec = { ...DEFAULT_SPEC, ...p.spec };
            this._syncControls();
            this._changed();
          }
        }
        this._renderPresets();
      });
    });
  }

  /** Mọi thay đổi state -> vẽ lại đúng 1 lần (mục 79). */
  _changed() {
    this._syncControls();
    this._drawPreview();
    this._scheduleRealPreview();
  }

  /* ---------------- Preview (canvas — cùng hình học backend) ---------------- */

  _drawPreview() {
    const canvas = this.el.querySelector('#vs-canvas');
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const W = canvas.width;
    const S = 16;                    // texture space
    const scale = W / S;
    ctx.clearRect(0, 0, W, W);

    // Nền Minecraft-like: grid tối (mục 8.2 background grid)
    const css = getComputedStyle(document.documentElement);
    ctx.fillStyle = css.getPropertyValue('--bg-1').trim() || '#0d0f14';
    ctx.fillRect(0, 0, W, W);
    ctx.strokeStyle = css.getPropertyValue('--border-0').trim() || 'rgba(255,255,255,.06)';
    ctx.lineWidth = 1;
    for (let i = 1; i < 4; i++) {
      ctx.beginPath(); ctx.moveTo(i * W / 4, 0); ctx.lineTo(i * W / 4, W); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(0, i * W / 4); ctx.lineTo(W, i * W / 4); ctx.stroke();
    }

    const s = this._spec;
    const rgb = this._hex(s.color);
    const oRgb = this._hex(s.outlineColor);
    const alpha = Math.round(255 * s.opacity);

    const put = (x, y, c, a) => {
      if (x < 0 || y < 0 || x >= S || y >= S) return;
      ctx.fillStyle = `rgba(${c[0]},${c[1]},${c[2]},${a / 255})`;
      ctx.fillRect(x * scale, y * scale, scale, scale);
    };

    const mid = S / 2, t = s.thickness, gap = s.gap;
    const main = [];
    if (s.shape === 'dot') {
      for (let dy = -Math.floor(t / 2); dy <= Math.floor(t / 2); dy++)
        for (let dx = -Math.floor(t / 2); dx <= Math.floor(t / 2); dx++)
          main.push([mid + dx, mid + dy, rgb, alpha]);
    } else {
      for (let i = gap; i < mid; i++) {
        for (let k = 0; k < t; k++) {
          main.push([mid + k, mid - 1 - i, rgb, alpha]);
          main.push([mid + k, mid + i, rgb, alpha]);
          main.push([mid - 1 - i, mid + k, rgb, alpha]);
          main.push([mid + i, mid + k, rgb, alpha]);
        }
      }
      if (s.shape === 'circle') {
        const r = mid - gap / 2;
        for (let step = 0; step < 72; step++) {
          const ang = step * Math.PI / 36;
          const x = Math.floor(mid + r * Math.cos(ang) - t / 2);
          const y = Math.floor(mid + r * Math.sin(ang) - t / 2);
          for (let k = 0; k < t; k++) for (let j = 0; j < t; j++)
            main.push([x + j, y + k, rgb, alpha]);
        }
      }
    }

    if (s.outline) {
      for (const [x, y] of main)
        for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]])
          put(x + dx, y + dy, oRgb, alpha);
    }
    for (const [x, y, c, a] of main) put(x, y, c, a);

    if (s.dot) {
      const r = Math.floor((t - 1) / 2);
      for (let dy = -r; dy <= r; dy++)
        for (let dx = -r; dx <= r; dx++)
          put(mid + dx, mid + dy, rgb, alpha);
    }
  }

  /** Preview PNG thật từ backend (đảm bảo khớp pixel pack) — debounce 300ms. */
  _scheduleRealPreview() {
    if (this._previewTimer) clearTimeout(this._previewTimer);
    this._previewTimer = setTimeout(async () => {
      const res = await call('visual_render_preview', this._spec);
      if (!res?.ok || !this.el.isConnected) return;
      const img = this.el.querySelector('#vs-real-img');
      if (img) img.src = res.data.preview;
    }, 300);
  }

  _hex(c) {
    const m = String(c || '#ffffff').replace('#', '');
    const f = m.length === 3 ? m.split('').map((x) => x + x).join('') : m;
    const n = parseInt(f, 16) || 0xffffff;
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }

  /* ---------------- Export (mục 8.3) ---------------- */

  async _export() {
    const instances = await call('instances_list');
    const list = instances?.ok ? instances.data.instances : [];
    const versions = ['1.21.4', '1.21.1', '1.21', '1.20.6', '1.20.4', '1.20.1', '1.19.4', '1.18.2'];
    const m = openModal({
      title: t('vs.exportTitle'),
      icon: 'check',
      body: `
        <div class="field"><label>${t('vs.name')}</label>
          <input type="text" id="vs-exp-name" value="${esc(t('vs.defaultName'))}" /></div>
        <div class="field"><label>${t('rs.version')}</label>
          <select id="vs-exp-version">${versions.map((v) =>
            `<option value="${v}"${v === '1.21.4' ? ' selected' : ''}>${v}</option>`).join('')}</select></div>
        <div class="field"><label>${t('rs.installTitle')}</label>
          <select id="vs-exp-target">
            <option value="">${t('vs.noInstall')}</option>
            ${list.map((i) => `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join('')}
          </select></div>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('vs.export'), variant: 'primary', autofocus: true,
          onClick: async ({ close }) => {
            const name = m.el.querySelector('#vs-exp-name')?.value?.trim();
            const version = m.el.querySelector('#vs-exp-version')?.value;
            const target = m.el.querySelector('#vs-exp-target')?.value || null;
            if (!name) { toastError(t('rs.error.name')); return; }
            const res = await call('visual_export_pack', name, this._spec, version, target);
            if (!res?.ok) { handleError(res.error); return; }
            close();
            toastSuccess(t('vs.toast.exported'));
          },
        },
      ],
    });
    queueMicrotask(() => m.el.querySelector('#vs-exp-name')?.focus());
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    if (this._previewTimer) clearTimeout(this._previewTimer);
  }
}
