/**
 * Presets hub (mục 52 "Presets"): gallery tổng hợp preset crosshair + totem
 * (mỗi preset render preview thật qua backend), bấm = nhảy sang studio
 * tương ứng với preset đã chọn.
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { handleError } from '../../app/errors.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

export class PresetsHubView {
  constructor(el) {
    this.el = el;
  }

  async mount() {
    this.el.innerHTML = `
      <div class="card section" style="margin-top:0">
        <h3>${icon('star', 14)} ${t('vs.pr.title')}</h3>
        <div class="muted">${t('vs.pr.subtitle')}</div>
        <div id="pr-gallery">${t('common.loading')}</div>
      </div>`;

    const host = this.el.querySelector('#pr-gallery');
    try {
      const [ch, to] = await Promise.all([
        call('visual_presets'),
        call('visual_totem_presets'),
      ]);
      if (!ch?.ok || !to?.ok) throw new Error(ch?.error?.message || to?.error?.message);
      const chList = (ch.data.presets || []).slice(0, 7);
      const toList = (to.data.presets || []).slice(0, 8);

      // crosshair: preview PNG từ spec (render_preview)
      const chPreviews = await Promise.all(chList.map((p) =>
        call('visual_render_preview', p.spec).then((r) => r?.ok ? r.data.preview : null)));
      const toPreviews = await Promise.all(toList.map((p) =>
        call('visual_render_totem', p.spec).then((r) => r?.ok ? r.data.preview : null)));

      host.innerHTML = `
        <div class="pr-section">${t('vs.tab.crosshair')}</div>
        <div class="pr-grid">${chList.map((p, i) => this._card('crosshair', p, chPreviews[i])).join('')}</div>
        <div class="pr-section">${t('vs.tab.totem')}</div>
        <div class="pr-grid">${toList.map((p, i) => this._card('totem', p, toPreviews[i])).join('')}</div>`;

      host.querySelectorAll('[data-studio]').forEach((card) => {
        card.addEventListener('click', () => this._openStudio(card.dataset.studio, card.dataset.preset));
      });
    } catch (e) {
      handleError(e);
      host.innerHTML = `<div class="empty-sub">${t('common.error')}</div>`;
    }
  }

  unmount() { /* không có subscription */ }

  _card(studio, preset, preview) {
    return `
      <button class="pr-card" data-studio="${studio}" data-preset="${esc(preset.id)}">
        <img src="${preview || ''}" alt="${esc(preset.id)}" />
        <span class="pr-name">${esc(t(`vs.${studio === 'crosshair' ? 'ch' : 'to'}.preset.${preset.id}`))}</span>
      </button>`;
  }

  async _openStudio(studio, presetId) {
    // Đổi subtab active + yêu cầu studio áp preset (qua localStorage event-lite —
    // shell render lại, studio đọc param từ router state)
    try {
      sessionStorage.setItem('antares.preset', JSON.stringify({ studio, presetId }));
      // click subtab tương ứng trong shell
      const shell = this.el.closest('.view');
      const tabs = shell?.querySelectorAll('[data-subtab]');
      if (tabs) {
        for (const b of tabs) {
          if (b.dataset.subtab === studio) { b.click(); break; }
        }
      }
    } catch { /* sessionStorage không sẵn sàng — chỉ đổi tab */ }
  }
}
