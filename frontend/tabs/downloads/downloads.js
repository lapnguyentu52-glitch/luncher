/** Downloads tab — task center: active + finished (mục 149, 150). */
import { call } from '../../app/bridge.js';
import { toastSuccess } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { onBackendEvent } from '../../app/events.js';

const STATE_TONE = {
  running: 'accent dot',
  pending: 'warn dot',
  completed: 'online dot',
  failed: 'offline dot',
  cancelled: '',
};

export class DownloadsView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._timer = null;
  }

  /** Gộp nhiều event liên tiếp thành 1 lần đọc danh sách. */
  _refreshSoon() {
    if (this._timer) clearTimeout(this._timer);
    this._timer = setTimeout(() => { this._timer = null; this._refresh(); }, 150);
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('downloads.title')}</div>
          <div class="view-sub">${t('downloads.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="dl-refresh">${icon('refresh', 15, 'btn-icon')}${t('logs.refresh')}</button>
        </div>
      </div>

      <div class="card section" style="margin-top:0">
        <h3>${icon('downloads', 14)} ${t('downloads.active')}</h3>
        <div id="dl-active" class="list"></div>
      </div>

      <div class="card section">
        <h3>${icon('clock', 14)} ${t('downloads.history')}</h3>
        <div id="dl-history" class="list"></div>
      </div>`;

    this.el.querySelector('#dl-refresh')?.addEventListener('click', () => this._refresh());
    await this._refresh();

    // Không còn poll mỗi giây — task.updated/download.* đẩy từ backend (mục 15.2)
    this._unsubs.push(
      onBackendEvent('task.updated', () => this._refreshSoon()),
      onBackendEvent('download.*', () => this._refreshSoon()),
    );
  }

  async _refresh() {
    const res = await call('tasks_list');
    if (!res?.ok) return;
    const tasks = res.data.tasks || [];
    const active = tasks.filter((x) => x.state === 'pending' || x.state === 'running');
    const history = tasks.filter((x) => x.state !== 'pending' && x.state !== 'running');

    const activeBox = this.el.querySelector('#dl-active');
    const historyBox = this.el.querySelector('#dl-history');
    if (!activeBox || !historyBox) return;

    activeBox.innerHTML = active.length === 0
      ? emptyRow('downloads.empty')
      : active.map((x) => this._activeRow(x)).join('');

    historyBox.innerHTML = history.length === 0
      ? emptyRow('downloads.empty')
      : history.slice(0, 30).map((x) => this._historyRow(x)).join('');

    activeBox.querySelectorAll('[data-cancel]').forEach((b) =>
      b.addEventListener('click', async () => {
        const r = await call('tasks_cancel', b.dataset.cancel);
        if (!r.ok) { handleError(r.error); return; }
        toastSuccess(t('downloads.cancelled'));
        this._refresh();
      }));
  }

  _activeRow(task) {
    return `
      <div class="scan-row" style="flex-direction:column;align-items:stretch">
        <div style="display:flex;gap:8px;align-items:center;width:100%">
          <span class="badge ${STATE_TONE[task.state] || ''}">${esc(t('downloads.state.' + task.state))}</span>
          <span class="grow"><b>${esc(task.type)}</b>
            <span class="muted">${esc(task.owner || '')}</span></span>
          <span class="muted">${Math.round(task.progress || 0)}%</span>
          <button class="btn ghost sm" data-cancel="${esc(task.id)}">${t('downloads.cancel')}</button>
        </div>
        <div class="progress thin" style="margin-top:8px"><div style="width:${task.progress || 0}%"></div></div>
        ${task.message ? `<div class="muted" style="margin-top:6px">${esc(task.message)}</div>` : ''}
      </div>`;
  }

  _historyRow(task) {
    return `
      <div class="row">
        <span class="badge ${STATE_TONE[task.state] || ''}">${esc(t('downloads.state.' + task.state))}</span>
        <span class="grow truncate">${esc(task.type)}<span class="muted"> · ${esc(task.message || task.owner || '')}</span></span>
      </div>`;
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    if (this._timer) clearTimeout(this._timer);
    this._timer = null;
  }
}

function emptyRow(key) {
  return `<div class="empty">${icon('downloads', 26)}<div>${t(key)}</div></div>`;
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
