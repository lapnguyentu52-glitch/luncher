/**
 * Performance tab — Performance Center (spec 3.0 mục 5, 55, 89).
 *
 * - Metric cards: CPU / RAM / Disk I/O / Launcher overhead.
 * - Mỗi card: giá trị hiện tại + sparkline (mục 55: Current + Trend).
 * - Dữ liệu realtime từ performance-store (backend đẩy, KHÔNG poll — mục 5.2).
 * - Game mode badge: khi Minecraft chạy, backend tự giảm nhịp sampler (mục 30).
 */
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { Sparkline } from '../../app/chart.js';
import * as store from '../../state/performance-store.js';

const fmtMb = (mb) => (mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`);

export class PerformanceView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._charts = {};
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('performance.title')}</div>
          <div class="view-sub">${t('performance.subtitle')}</div>
        </div>
        <div class="page-actions">
          <span class="badge ${this._gameOn ? 'accent' : ''}" id="perf-gamemode">
            ${icon('rocket', 13)}<span id="perf-gamemode-text"></span>
          </span>
        </div>
      </div>

      <div class="perf-grid">
        ${this._card('cpu', 'cpu', 'perf-cpu-val', '%')}
        ${this._card('ram', 'memory', 'perf-ram-val', '')}
        ${this._card('disk', 'database', 'perf-disk-val', '')}
        ${this._card('launcher', 'star', 'perf-launcher-val', '')}
      </div>`;

    const make = (id, color, opts) => {
      const canvas = this.el.querySelector(`#${id}`);
      return canvas ? new Sparkline(canvas, { color, ...opts }) : null;
    };
    this._charts.cpu = make('chart-cpu', '#5b9dff', { max: 100 });
    this._charts.ram = make('chart-ram', '#35e08f', { max: 100 });
    this._charts.disk = make('chart-disk', '#c88a3c');
    this._charts.launcher = make('chart-launcher', '#b06fe0');

    await store.seed();
    this._unsubs.push(store.onChange((s) => this._render(s)));
    this._render(store.getState());
  }

  _card(key, ic, valId, unit) {
    return `
      <div class="card perf-card">
        <div class="perf-card-head">
          <span class="perf-card-icon">${icon(ic, 16)}</span>
          <span class="perf-card-title">${t(`performance.card.${key}`)}</span>
        </div>
        <div class="perf-card-value"><span id="${valId}">—</span><small>${unit}</small></div>
        <canvas class="perf-spark" id="chart-${key}"></canvas>
        <div class="perf-card-foot muted" id="foot-${key}"></div>
      </div>`;
  }

  _render(s) {
    if (!this.el.isConnected) return;

    // Game mode badge
    const gmText = this.el.querySelector('#perf-gamemode-text');
    const gmBadge = this.el.querySelector('#perf-gamemode');
    if (gmText) gmText.textContent = t(s.gameMode ? 'performance.gameModeOn' : 'performance.gameModeOff');
    if (gmBadge) gmBadge.classList.toggle('accent', s.gameMode);

    const last = s.latest;
    if (!last) return;

    // CPU
    const cpuVal = this.el.querySelector('#perf-cpu-val');
    if (cpuVal) cpuVal.textContent = String(Math.round(last.cpu ?? 0));
    this._charts.cpu?.setData(s.history.cpu.slice(-60));
    this._foot('cpu', last.cpu != null
      ? t('performance.foot.cores', { count: this._cores || '—' }) : '');

    // RAM
    const ramVal = this.el.querySelector('#perf-ram-val');
    if (ramVal) ramVal.textContent = `${Math.round(last.ram?.percent ?? 0)}`;
    this._charts.ram?.setData(s.history.ram.slice(-60));
    this._foot('ram', last.ram
      ? `${fmtMb(last.ram.used / (1024 * 1024))} / ${fmtMb(last.ram.total / (1024 * 1024))}` : '');

    // Disk
    const diskVal = this.el.querySelector('#perf-disk-val');
    if (diskVal) {
      const r = last.disk?.readBps || 0;
      const w = last.disk?.writeBps || 0;
      diskVal.textContent = r + w > 0 ? fmtMb((r + w) / (1024 * 1024)) + '/s' : '—';
    }
    this._charts.disk?.setData(s.history.diskRead.slice(-60).map((p, i) =>
      [p[0], p[1] + (s.history.diskWrite.slice(-60)[i]?.[1] || 0)]));
    this._foot('disk', last.disk
      ? `↓ ${fmtMb((last.disk.readBps || 0) / (1024 * 1024))}/s · ↑ ${fmtMb((last.disk.writeBps || 0) / (1024 * 1024))}/s` : '');

    // Launcher overhead (mục 29 KPI)
    const lVal = this.el.querySelector('#perf-launcher-val');
    if (lVal) lVal.textContent = String(Math.round(last.launcher?.cpu ?? 0));
    this._charts.launcher?.setData(s.history.launcherRss.slice(-60));
    this._foot('launcher', last.launcher
      ? `RSS ${fmtMb((last.launcher.rss || 0) / (1024 * 1024))}` : '');
  }

  _foot(key, text) {
    const el = this.el.querySelector(`#foot-${key}`);
    if (el && text) el.textContent = text;
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._charts = {}; // GC canvas contexts — không giữ renderer khi rời tab (mục 78)
  }
}
