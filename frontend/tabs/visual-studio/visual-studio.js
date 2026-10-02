/**
 * Visual Studio shell (spec 3.0 mục 52: Crosshair / Totem / HUD / FX / Export).
 * Mỗi studio là view class riêng; shell chỉ quản lý subtab + mount/unmount
 * (đúng 1 view active — destroy listeners khi rời — mục 26.3).
 */
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { CrosshairStudioView } from './crosshair.js';
import { TotemStudioView } from './totem.js';
import { Totem3DView } from './totem-3d.js';
import { HudStudioView } from './hud.js';
import { FxStudioView } from './fx.js';
import { PresetsHubView } from './presets.js';

const SUBTABS = [
  { id: 'crosshair', labelKey: 'vs.tab.crosshair', icon: 'edit', cls: CrosshairStudioView },
  { id: 'totem', labelKey: 'vs.tab.totem', icon: 'sparkle', cls: TotemStudioView },
  { id: 'totem3d', labelKey: 'vs.tab.totem3d', icon: 'cube', cls: Totem3DView },
  { id: 'hud', labelKey: 'vs.tab.hud', icon: 'dashboard', cls: HudStudioView },
  { id: 'fx', labelKey: 'vs.tab.fx', icon: 'diagnostics', cls: FxStudioView },
  { id: 'presets', labelKey: 'vs.tab.presets', icon: 'star', cls: PresetsHubView },
];

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

export class VisualStudioView {
  constructor(el) {
    this.el = el;
    this._active = 'crosshair';
    this._view = null;
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('vs.title')}</div>
          <div class="view-sub">${t('vs.subtitle')}</div>
        </div>
      </div>
      <div class="tabs" id="vs-subtabs" role="tablist">
        ${SUBTABS.map((s) => `
          <button class="tab${s.id === this._active ? ' active' : ''}" role="tab"
                  data-subtab="${s.id}" aria-selected="${s.id === this._active}">
            ${icon(s.icon, 14)} ${esc(t(s.labelKey))}
          </button>`).join('')}
      </div>
      <div id="vs-subview"></div>`;

    this.el.querySelector('#vs-subtabs').addEventListener('click', (e) => {
      const btn = e.target.closest('[data-subtab]');
      if (btn) this._switch(btn.dataset.subtab);
    });

    await this._mountActive();
  }

  async _switch(id) {
    if (id === this._active) return;
    this._active = id;
    this.el.querySelectorAll('[data-subtab]').forEach((b) => {
      const on = b.dataset.subtab === id;
      b.classList.toggle('active', on);
      b.setAttribute('aria-selected', String(on));
    });
    await this._mountActive();
  }

  async _mountActive() {
    const host = this.el.querySelector('#vs-subview');
    if (!host) return;
    // Unmount view cũ — dọn timer/listener (mục 26.3)
    this._view?.unmount?.();
    this._view = null;
    host.innerHTML = '';
    const meta = SUBTABS.find((s) => s.id === this._active) || SUBTABS[0];
    this._view = new meta.cls(host);
    await this._view.mount();
    this._applyPendingPreset();
  }

  /** Presets hub handoff: đọc sessionStorage -> click chip preset tương ứng. */
  _applyPendingPreset() {
    try {
      const raw = sessionStorage.getItem('antares.preset');
      if (!raw) return;
      const { studio, presetId } = JSON.parse(raw);
      sessionStorage.removeItem('antares.preset');
      if (studio !== this._active) return;
      // Cả crosshair/totem đều render chip [data-preset] — click để áp
      this._view?.el?.querySelector(`[data-preset="${presetId}"]`)?.click();
    } catch { /* storage không sẵn sàng */ }
  }

  unmount() {
    this._view?.unmount?.();
    this._view = null;
  }
}
