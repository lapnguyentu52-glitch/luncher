/**
 * Pack Layers panel (master plan v3 mục 25 Batch 7, UI mục 25.7).
 *
 * Chọn instance -> layer order (thấp → cao priority) -> reorder ↑↓ ->
 * preview effective assets + conflicts. Thứ tự sync vào options.txt
 * (pack cao priority cuối cùng = thắng path trùng).
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

/** Render panel vào host. @param {HTMLElement} host @param {Function} onChanged */
export async function renderLayersPanel(host, instances, onChanged) {
  if (!host) return;
  host.innerHTML = `
    <h3>${icon('layers', 14)} ${esc(t('ly.title'))}</h3>
    <div class="field"><label>${esc(t('opt.instance'))}</label>
      <select id="ly-inst">${instances.map((i) =>
        `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join('')}</select></div>
    <div id="ly-body" class="muted">${esc(t('common.loading'))}</div>`;

  const sel = host.querySelector('#ly-inst');
  sel.addEventListener('change', () => _load(host, sel.value, onChanged));
  await _load(host, sel.value, onChanged);
}

async function _load(host, instanceId, onChanged) {
  const body = host.querySelector('#ly-body');
  if (!body || !instanceId) return;
  const res = await call('resource_layer_get', instanceId);
  if (!res?.ok) { handleError(res.error); return; }
  const { order, preview } = res.data;

  const conflictCount = preview.conflicts?.length || 0;
  body.innerHTML = `
    ${order.length ? `<div class="list">${order.map((f, i) => `
      <div class="row" data-file="${esc(f)}">
        <span class="grow truncate" title="${esc(f)}"><b>${i + 1}.</b> ${esc(f)}</span>
        <button class="icon-btn plain" data-mv="-1" ${i === 0 ? 'disabled' : ''}
                title="${esc(t('ly.up'))}" aria-label="${esc(t('ly.up'))}">${icon('arrowLeft', 13)}</button>
        <button class="icon-btn plain" data-mv="1" ${i === order.length - 1 ? 'disabled' : ''}
                title="${esc(t('ly.down'))}" aria-label="${esc(t('ly.down'))}">${icon('arrowRight', 13)}</button>
      </div>`).join('')}</div>
      <div class="muted" style="margin:6px 0">${esc(t('ly.hint'))}</div>`
      : `<div class="muted">${esc(t('ly.empty'))}</div>`}
    ${conflictCount ? `<div class="ly-conflicts">
      <b>${esc(t('ly.conflicts', { count: conflictCount }))}</b>
      ${preview.conflicts.slice(0, 5).map((c) => `
        <div class="row"><span class="grow truncate mono">${esc(c.path)}</span>
          <span class="muted truncate">${esc(c.packs.join(' < '))}</span></div>`).join('')}
      ${conflictCount > 5 ? `<div class="muted">+${conflictCount - 5}…</div>` : ''}
    </div>` : ''}`;

  body.querySelectorAll('[data-mv]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const row = btn.closest('[data-file]');
      const file = row.dataset.file;
      const res2 = await call('resource_layer_move', instanceId, file, +btn.dataset.mv);
      if (!res2?.ok) { handleError(res2.error); return; }
      await _load(host, instanceId, onChanged);
      onChanged?.();
    });
  });
}
