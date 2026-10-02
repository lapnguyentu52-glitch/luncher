/**
 * Totem Studio subtab (spec 3.0 mục 9).
 *
 * Preview texture 32x32 (zoom 8x, pixelated) + color controls + 8 preset
 * (mục 9.4) + export qua pipeline Resource Studio. Preview PNG backend
 * đảm bảo khớp pixel pack (như crosshair).
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

const DEFAULT_SPEC = { base: '#e8b23a', accent: '#8c5a2b', eye: '#3ad0e8', glow: false, wing: true };
const PRESET_IDS = ['classic', 'crystal', 'minimal', 'dark', 'red', 'glass', 'lowpoly', 'competitive'];
const COLOR_FIELDS = [
  ['base', 'vs.to.base'],
  ['accent', 'vs.to.accent'],
  ['eye', 'vs.to.eye'],
];

export class TotemStudioView {
  constructor(el) {
    this.el = el;
    this._spec = { ...DEFAULT_SPEC };
    this._presetId = 'classic';
    this._timer = null;
  }

  async mount() {
    this.el.innerHTML = `
      <div class="vs-layout">
        <div class="card section vs-preview-card" style="margin-top:0">
          <h3>${icon('sparkle', 14)} ${t('vs.preview')}</h3>
          <div class="vs-canvas-wrap vs-totem-wrap">
            <img id="to-preview" width="256" height="256" alt="${esc(t('vs.preview'))}" />
          </div>
          <div class="muted" style="margin-top:8px">${t('vs.to.previewHint')}</div>
        </div>

        <div class="card section" style="margin-top:0">
          <h3>${icon('edit', 14)} ${t('vs.controls')}</h3>
          <div class="field"><label>${t('vs.preset')}</label>
            <div id="to-presets" class="opt-profiles"></div></div>
          ${COLOR_FIELDS.map(([key, label]) => `
            <div class="field"><label>${t(label)}</label>
              <input type="color" id="to-${key}" /></div>`).join('')}
          <div class="field">
            <label class="switch"><input type="checkbox" id="to-wing" />
              <span class="track"></span>${t('vs.to.wing')}</label></div>
          <div class="field">
            <label class="switch"><input type="checkbox" id="to-glow" />
              <span class="track"></span>${t('vs.to.glow')}</label></div>
          <button class="btn block" id="to-advanced">${icon('cube', 15, 'btn-icon')}${t('vs.to.advanced')}</button>
          <button class="btn primary block" id="to-export">${icon('check', 15, 'btn-icon')}${t('vs.export')}</button>
        </div>
      </div>`;

    this.el.querySelector('#to-presets').addEventListener('click', (e) => {
      const chip = e.target.closest('[data-preset]');
      if (chip) this._applyPreset(chip.dataset.preset);
    });
    for (const [key] of COLOR_FIELDS) {
      this.el.querySelector(`#to-${key}`).addEventListener('input', (e) => {
        this._spec[key] = e.target.value;
        this._presetId = null;
        this._renderPresets();
        this._changed();
      });
    }
    this.el.querySelector('#to-wing').addEventListener('change', (e) => {
      this._spec.wing = e.target.checked; this._changed();
    });
    this.el.querySelector('#to-glow').addEventListener('change', (e) => {
      this._spec.glow = e.target.checked; this._changed();
    });
    this.el.querySelector('#to-export').addEventListener('click', () => this._export());
    // mục 70: Quick -> Advanced — truyền màu Quick qua handoff (Batch 8)
    this.el.querySelector('#to-advanced')?.addEventListener('click', () => {
      try {
        sessionStorage.setItem('antares.totem3d.colors', JSON.stringify({
          base: this._spec.base, accent: this._spec.accent, eye: this._spec.eye,
        }));
      } catch { /* no-op */ }
      const tabs = this.el.closest('#vs-subview')?.parentElement?.querySelector('#vs-subtabs');
      tabs?.querySelector('[data-subtab="totem3d"]')?.click();
    });

    await this._applyPreset('classic');
  }

  async _applyPreset(id) {
    const res = await call('visual_totem_presets');
    if (!res?.ok) return;
    const p = (res.data.presets || []).find((x) => x.id === id);
    if (p) {
      this._spec = { ...DEFAULT_SPEC, ...p.spec };
      this._presetId = id;
      this._sync();
    }
    this._renderPresets();
  }

  _renderPresets() {
    const wrap = this.el.querySelector('#to-presets');
    if (!wrap) return;
    wrap.innerHTML = PRESET_IDS.map((id) => `
      <button class="chip${id === this._presetId ? ' active' : ''}" data-preset="${id}">
        ${esc(t('vs.to.preset.' + id))}
      </button>`).join('');
  }

  _sync() {
    for (const [key] of COLOR_FIELDS) {
      const input = this.el.querySelector(`#to-${key}`);
      if (input) input.value = this._spec[key];
    }
    this.el.querySelector('#to-wing').checked = this._spec.wing;
    this.el.querySelector('#to-glow').checked = this._spec.glow;
  }

  _changed() {
    this._sync();
    if (this._timer) clearTimeout(this._timer);
    this._timer = setTimeout(async () => {
      const res = await call('visual_render_totem', this._spec);
      if (res?.ok) {
        const img = this.el.querySelector('#to-preview');
        if (img) img.src = res.data.preview;
      }
    }, 200);
  }

  async _export() {
    const instances = await call('instances_list');
    const list = instances?.ok ? instances.data.instances : [];
    const versions = ['1.21.4', '1.21.1', '1.21', '1.20.6', '1.20.4', '1.20.1', '1.19.4', '1.18.2'];
    const m = openModal({
      title: t('vs.to.exportTitle'),
      icon: 'check',
      body: `
        <div class="field"><label>${t('vs.name')}</label>
          <input type="text" id="to-exp-name" value="${esc(t('vs.to.defaultName'))}" /></div>
        <div class="field"><label>${t('rs.version')}</label>
          <select id="to-exp-version">${versions.map((v) =>
            `<option value="${v}"${v === '1.21.4' ? ' selected' : ''}>${v}</option>`).join('')}</select></div>
        <div class="field"><label>${t('rs.installTitle')}</label>
          <select id="to-exp-target">
            <option value="">${t('vs.noInstall')}</option>
            ${list.map((i) => `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join('')}
          </select></div>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('vs.export'), variant: 'primary', autofocus: true,
          onClick: async ({ close }) => {
            const name = m.el.querySelector('#to-exp-name')?.value?.trim();
            const version = m.el.querySelector('#to-exp-version')?.value;
            const target = m.el.querySelector('#to-exp-target')?.value || null;
            if (!name) { toastError(t('rs.error.name')); return; }
            const res = await call('visual_export_totem_pack', name, this._spec, version, target);
            if (!res?.ok) { handleError(res.error); return; }
            close();
            toastSuccess(t('vs.toast.exported'));
          },
        },
      ],
    });
  }

  unmount() {
    if (this._timer) clearTimeout(this._timer);
  }
}
