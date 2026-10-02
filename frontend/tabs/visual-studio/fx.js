/**
 * FX Studio subtab (spec 3.0 mục 52: Hit Effects + Particles).
 *
 * Hit overlay: kind/color/alpha/size — preview PNG backend khớp pixel pack.
 * Particles: shape/color/glow/frames — preview frame đầu (frames=1);
 * export ghi atlas đầy đủ + .mcmeta animation (chuẩn MC, dọc 1 cột 16px).
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { openModal } from '../../app/modal.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

export class FxStudioView {
  constructor(el) {
    this.el = el;
    this._defaults = null;
    this._hit = null;
    this._particle = null;
    this._timer = null;
  }

  async mount() {
    const res = await call('visual_fx_defaults');
    if (!res?.ok) { handleError(res?.error || new Error('fx defaults failed')); return; }
    this._defaults = res.data;
    this._hit = { ...this._defaults.hit.default };
    this._particle = { ...this._defaults.particle.default };

    this.el.innerHTML = `
      <div class="vs-layout">
        <div class="card section vs-preview-card" style="margin-top:0">
          <h3>${icon('diagnostics', 14)} ${t('vs.fx.hitPreview')}</h3>
          <div class="vs-canvas-wrap">
            <img id="fx-hit-preview" width="256" height="256" alt="hit" />
          </div>
          <h3 style="margin-top:14px">${icon('sparkle', 14)} ${t('vs.fx.particlePreview')}</h3>
          <div class="vs-canvas-wrap" style="width:128px;height:128px">
            <img id="fx-particle-preview" width="128" height="128" alt="particle" />
          </div>
          <div class="muted" style="margin-top:8px">${t('vs.fx.previewHint')}</div>
        </div>

        <div class="card section" style="margin-top:0">
          <h3>${icon('edit', 14)} ${t('vs.fx.hitTitle')}</h3>
          <div class="field"><label>${t('vs.fx.kind')}</label>
            <div id="fx-hit-kinds" class="opt-profiles"></div></div>
          <div class="field"><label>${t('vs.fx.color')}</label>
            <input type="color" id="fx-hit-color" /></div>
          <div class="field"><label>${t('vs.fx.alpha')} <span id="fx-hit-alpha-v"></span></label>
            <input type="range" id="fx-hit-alpha" min="0" max="200" step="5" /></div>
          <div class="field"><label>${t('vs.fx.size')} <span id="fx-hit-size-v"></span></label>
            <input type="range" id="fx-hit-size" min="20" max="100" step="5" /></div>

          <h3 style="margin-top:14px">${icon('edit', 14)} ${t('vs.fx.particleTitle')}</h3>
          <div class="field"><label>${t('vs.fx.shape')}</label>
            <div id="fx-p-shapes" class="opt-profiles"></div></div>
          <div class="field"><label>${t('vs.fx.color')}</label>
            <input type="color" id="fx-p-color" /></div>
          <div class="field">
            <label class="switch"><input type="checkbox" id="fx-p-glow" />
              <span class="track"></span>${t('vs.fx.glow')}</label></div>
          <div class="field"><label>${t('vs.fx.frames')} <span id="fx-p-frames-v"></span></label>
            <input type="range" id="fx-p-frames" min="1" max="8" step="1" /></div>

          <button class="btn primary block" id="fx-export">${icon('check', 15, 'btn-icon')}${t('vs.export')}</button>
        </div>
      </div>`;

    this._bindControls();
    await Promise.all([this._renderHit(), this._renderParticle()]);
  }

  unmount() {
    if (this._timer) { clearTimeout(this._timer); this._timer = null; }
  }

  _bindControls() {
    // ---- hit ----
    const kinds = this.el.querySelector('#fx-hit-kinds');
    kinds.innerHTML = this._defaults.hit.kinds.map((k) =>
      `<button class="chip${k === this._hit.kind ? ' active' : ''}" data-kind="${k}">${esc(t('vs.fx.kind.' + k))}</button>`).join('');
    kinds.addEventListener('click', (e) => {
      const chip = e.target.closest('[data-kind]');
      if (!chip) return;
      this._hit.kind = chip.dataset.kind;
      kinds.querySelectorAll('.chip').forEach((c) =>
        c.classList.toggle('active', c.dataset.kind === this._hit.kind));
      this._renderHit();
    });

    const hitColor = this.el.querySelector('#fx-hit-color');
    hitColor.value = this._hit.color;
    hitColor.addEventListener('input', () => {
      this._hit.color = hitColor.value; this._renderHit();
    });
    for (const key of ['alpha', 'size']) {
      const input = this.el.querySelector(`#fx-hit-${key}`);
      input.value = this._hit[key];
      this.el.querySelector(`#fx-hit-${key}-v`).textContent = this._hit[key];
      input.addEventListener('input', () => {
        this._hit[key] = Number(input.value);
        this.el.querySelector(`#fx-hit-${key}-v`).textContent = input.value;
        this._renderHit();
      });
    }

    // ---- particle ----
    const shapes = this.el.querySelector('#fx-p-shapes');
    shapes.innerHTML = this._defaults.particle.shapes.map((sh) =>
      `<button class="chip${sh === this._particle.shape ? ' active' : ''}" data-shape="${sh}">${esc(t('vs.fx.shape.' + sh))}</button>`).join('');
    shapes.addEventListener('click', (e) => {
      const chip = e.target.closest('[data-shape]');
      if (!chip) return;
      this._particle.shape = chip.dataset.shape;
      shapes.querySelectorAll('.chip').forEach((c) =>
        c.classList.toggle('active', c.dataset.shape === this._particle.shape));
      this._renderParticle();
    });

    const pColor = this.el.querySelector('#fx-p-color');
    pColor.value = this._particle.color;
    pColor.addEventListener('input', () => {
      this._particle.color = pColor.value; this._renderParticle();
    });
    const glow = this.el.querySelector('#fx-p-glow');
    glow.checked = this._particle.glow;
    glow.addEventListener('change', () => {
      this._particle.glow = glow.checked; this._renderParticle();
    });
    const frames = this.el.querySelector('#fx-p-frames');
    frames.value = this._particle.frames;
    this.el.querySelector('#fx-p-frames-v').textContent = this._particle.frames;
    frames.addEventListener('input', () => {
      this._particle.frames = Number(frames.value);
      this.el.querySelector('#fx-p-frames-v').textContent = frames.value;
      this._renderParticle();
    });

    this.el.querySelector('#fx-export').addEventListener('click', () => this._export());
  }

  _debounced(fn) {
    if (this._timer) clearTimeout(this._timer);
    this._timer = setTimeout(async () => {
      this._timer = null;
      await fn();
    }, 150);
  }

  async _renderHit() {
    this._debounced(async () => {
      const res = await call('visual_render_hit', { ...this._hit });
      if (res?.ok) {
        const img = this.el.querySelector('#fx-hit-preview');
        if (img) img.src = res.data.preview;
      }
    });
  }

  async _renderParticle() {
    this._debounced(async () => {
      const res = await call('visual_render_particle', { ...this._particle });
      if (res?.ok) {
        const img = this.el.querySelector('#fx-particle-preview');
        if (img) img.src = res.data.preview;
      }
    });
  }

  async _export() {
    const instances = await call('instances_list');
    const list = instances?.ok ? instances.data.instances : [];
    const versions = ['1.21.4', '1.21.1', '1.21', '1.20.6', '1.20.4', '1.20.1', '1.19.4', '1.18.2'];
    const m = openModal({
      title: t('vs.fx.exportTitle'),
      icon: 'check',
      body: `
        <div class="field"><label>${t('vs.name')}</label>
          <input type="text" id="fx-exp-name" value="${esc(t('vs.fx.defaultName'))}" /></div>
        <div class="field"><label>${t('rs.version')}</label>
          <select id="fx-exp-version">${versions.map((v) =>
            `<option value="${v}"${v === '1.21.4' ? ' selected' : ''}>${v}</option>`).join('')}</select></div>
        <div class="field"><label>${t('rs.installTitle')}</label>
          <select id="fx-exp-target">
            <option value="">${t('vs.noInstall')}</option>
            ${list.map((i) => `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join('')}
          </select></div>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('vs.export'), variant: 'primary', autofocus: true,
          onClick: async ({ close }) => {
            const name = m.el.querySelector('#fx-exp-name')?.value?.trim();
            const version = m.el.querySelector('#fx-exp-version')?.value;
            const target = m.el.querySelector('#fx-exp-target')?.value || null;
            if (!name) { toastError(t('rs.error.name')); return; }
            const res = await call('visual_export_fx_pack', name,
              { ...this._hit }, { ...this._particle }, version, target);
            if (!res?.ok) { handleError(res.error); return; }
            close();
            toastSuccess(t('vs.toast.exported'));
          },
        },
      ],
    });
  }
}
