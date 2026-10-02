/**
 * Console tab — log launcher + Minecraft + crash-reports, ảo hoá (mục 16, 41).
 * Mới: chọn nguồn, insights phân tích lỗi (blueprint + khuyến nghị),
 * net check (TCP + Server List Ping) — phục vụ điều tra lỗi kết nối.
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { createVirtualLog } from '../../app/virtual-log.js';
import { onBackendEvents } from '../../app/events.js';

const LEVELS = ['all', 'INFO', 'WARNING', 'ERROR'];
const FETCH_LIMIT = 5000;   // khớp capacity ring buffer backend

export class LogsView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._view = null;
    this._filter = '';
    this._level = 'all';
    this._lines = [];        // [{text, level, logger, ts}] — meta từ backend
    this._shown = [];
    this._source = 'launcher';
    this._instanceId = null;
    this._stickEl = null;
  }

  async mount() {
    // Chọn instance cho nguồn minecraft/crash (dùng selectedInstance hiện tại)
    const state = await call('app_get_state');
    this._instanceId = state?.ok ? (state.data.selectedInstance || null) : null;

    const srcRes = await call('console_sources', this._instanceId);
    const sources = srcRes?.ok ? (srcRes.data.sources || []) : [];

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('logs.title')}</div>
          <div class="view-sub">${t('logs.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="console-analyze">${icon('search', 15, 'btn-icon')}${t('console.analyze')}</button>
          <button class="btn" id="console-net">${icon('wifi', 15, 'btn-icon')}${t('console.runNetCheck')}</button>
          <button class="btn" id="logs-refresh">${icon('refresh', 15, 'btn-icon')}${t('logs.refresh')}</button>
        </div>
      </div>

      <div class="card">
        <div class="toolbar" style="margin-bottom:12px">
          <div class="seg" id="console-sources">
            ${sources.map((s) => `
              <button data-src="${esc(s.id)}" data-ok="${s.available ? 1 : 0}"
                      class="${s.id === this._source ? 'active' : ''}"
                      data-tip="${esc(this._srcTip(s))}">
                ${esc(this._srcLabel(s.id))}${s.available ? '' : ' ·'}
              </button>`).join('')}
          </div>
          <input type="text" id="logs-filter" class="grow" placeholder="${esc(t('logs.filter'))}" />
          <div class="seg" id="logs-levels">
            ${LEVELS.map((l) => `<button data-level="${l}" class="${l === 'all' ? 'active' : ''}">
              ${l === 'all' ? esc(t('common.all')) : l}</button>`).join('')}
          </div>
        </div>

        <div id="console-insights"></div>

        <div class="console-wrap">
          <div class="console vlog" id="logs-view" style="height:420px"></div>
          <button class="console-jump" id="logs-jump" type="button">
            ${icon('chevronDown', 14)}${esc(t('logs.jumpToBottom'))}
          </button>
          <button class="console-jump" id="logs-resume" type="button" style="display:none">
            ${icon('chevronDown', 14)}${esc(t('servers.resumeFollow'))}<span class="muted" id="logs-newcount"></span>
          </button>
          <button class="console-clear" id="logs-clear" type="button"
                  title="${esc(t('servers.clearConsole'))}" aria-label="${esc(t('servers.clearConsole'))}">
            ${icon('eraser', 14)}${esc(t('servers.clearConsole'))}
          </button>
        </div>

        <div class="row-between" style="margin-top:10px">
          <span class="muted" id="logs-count"></span>
          <span class="muted" id="logs-follow"></span>
        </div>
      </div>

      <div class="card section" id="net-panel" hidden>
        <h3>${icon('wifi', 14)} ${t('console.netCheck')}</h3>
        <div class="toolbar" style="margin-bottom:10px">
          <input type="text" id="net-host" placeholder="${esc(t('console.netHost'))}" style="max-width:260px" />
          <input type="text" id="net-port" placeholder="${esc(t('console.netPort'))}" value="25565" style="max-width:110px" />
          <button class="btn primary" id="net-run">${icon('play', 14, 'btn-icon')}${t('console.netRun')}</button>
        </div>
        <div id="net-results" class="muted"></div>
      </div>`;

    const box = this.el.querySelector('#logs-view');
    this._view = createVirtualLog({
      container: box,
      emptyText: '—',
      classify: classifyLog,
    });
    this._stickEl = this.el.querySelector('#logs-jump');
    this._resumeEl = this.el.querySelector('#logs-resume');
    this._newCountEl = this.el.querySelector('#logs-newcount');
    this._pausedNew = 0;

    box.addEventListener('scroll', () => this._syncFollowUi(), { passive: true });
    this.el.querySelector('#logs-jump')?.addEventListener('click', () => {
      this._view.scrollToBottom();
      this._syncFollowUi();
    });
    this.el.querySelector('#logs-resume')?.addEventListener('click', () => {
      this._pausedNew = 0;
      this._view.scrollToBottom();
      this._syncFollowUi();
    });
    this.el.querySelector('#logs-clear')?.addEventListener('click', () => {
      this._lines = [];
      this._shown = [];
      this._pausedNew = 0;
      this._view.clear();
      this._applyFilters({ reset: true });
    });
    this.el.querySelector('#logs-filter')?.addEventListener('input', (e) => {
      this._filter = e.target.value.toLowerCase();
      this._applyFilters({ reset: true });
    });
    this.el.querySelector('#logs-levels')?.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-level]');
      if (!btn) return;
      this._level = btn.dataset.level;
      this.el.querySelectorAll('[data-level]').forEach((x) => x.classList.toggle('active', x === btn));
      this._applyFilters({ reset: true });
    });
    // Chuyển nguồn console: launcher = stream realtime, 2 nguồn kia = đọc file
    this.el.querySelector('#console-sources')?.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-src]');
      if (!btn || btn.dataset.src === this._source) return;
      this._source = btn.dataset.src;
      this.el.querySelectorAll('[data-src]').forEach((x) =>
        x.classList.toggle('active', x === btn));
      this._loadSource();
    });
    this.el.querySelector('#logs-refresh')?.addEventListener('click', () => this._loadSource());
    this.el.querySelector('#console-analyze')?.addEventListener('click', () => this._analyze());
    this.el.querySelector('#console-net')?.addEventListener('click', () => {
      const panel = this.el.querySelector('#net-panel');
      if (panel) panel.hidden = !panel.hidden;
    });
    this.el.querySelector('#net-run')?.addEventListener('click', () => this._runNetCheck());

    await this._loadSource();

    // Launcher log realtime (batch log.lines v2: lines = [{text, level, ...}]).
    // Chỉ áp khi đang xem nguồn launcher; 2 nguồn kia là file tĩnh, bấm refresh.
    this._unsubs.push(onBackendEvents(['log.lines', 'app.error'], (e) => {
      if (this._source !== 'launcher') return;
      const payload = e.payload || {};
      if (Array.isArray(payload.lines) && payload.lines.length) {
        const normalized = payload.lines.map((l) =>
          typeof l === 'string' ? { text: l, level: '' } : l);
        this._appendLines(normalized, Boolean(payload.reset));
      }
    }));
  }

  _srcLabel(id) {
    return { launcher: t('console.srcLauncher'),
             minecraft: t('console.srcMinecraft'),
             crash: t('console.srcCrash') }[id] || id;
  }

  _srcTip(s) {
    const base = this._srcLabel(s.id);
    if (s.id === 'crash' && s.count) return `${base} (${s.count})`;
    return base;
  }

  /** Nạp nguồn hiện tại: launcher = ring snapshot; khác = console_read. */
  async _loadSource() {
    if (this._source === 'launcher') {
      await this._fetch();
    } else {
      const res = await call('console_read', this._source, this._instanceId, FETCH_LIMIT);
      if (!res?.ok) { this._lines = []; this._applyFilters({ reset: true }); return; }
      this._lines = (res.data.lines || []).map((x) => ({ text: x.text, level: '' }));
      this._applyFilters({ reset: true });
    }
  }

  /** Gắn batch dòng mới: reset = ring buffer đã xoay -> nạp lại từ đầu. */
  _appendLines(batch, reset) {
    if (reset) {
      this._lines = batch.slice(-FETCH_LIMIT);
    } else {
      this._lines = this._lines.concat(batch);
      if (this._lines.length > FETCH_LIMIT) {
        this._lines = this._lines.slice(this._lines.length - FETCH_LIMIT);
      }
    }
    this._applyFilters();
  }

  /**
   * Nạp snapshot đầy đủ (lúc mở tab, refresh, re-focus).
   * Xong thì sync cursor về bridge để các batch `log.lines` kế tiếp nối tiếp
   * đúng sau dòng cuối — không trùng dòng đã nạp.
   */
  async _fetch() {
    const res = await call('logs_recent', FETCH_LIMIT);
    if (!res?.ok) return;
    this._lines = (res.data.lines || []).map((l) =>
      typeof l === 'string' ? { text: l, level: '' } : l);
    if (Number.isFinite(res.data.cursor)) {
      await call('events_sync_log_cursor', Math.round(res.data.cursor));
    }
    this._applyFilters({ reset: true });
  }

  _applyFilters({ reset = false } = {}) {
    let shown = this._lines;
    if (this._level !== 'all') {
      const want = this._level === 'ERROR'
        ? ['ERROR', 'FATAL', 'CRITICAL'] : [this._level];
      shown = shown.filter((l) => {
        if (l.level) return want.includes(l.level);
        // fallback: re-parse dòng cũ thiếu meta
        return want.some((w) => new RegExp(`\\b${w}\\b`).test(l.text));
      });
    }
    if (this._filter) shown = shown.filter((l) => l.text.toLowerCase().includes(this._filter));

    this._shown = shown;
    // Smart follow: đang cuộn lên đọc -> KHÔNG vẽ dòng mới, chỉ đếm.
    // UIX v3: render object {text, level} — tô màu dòng theo level metadata.
    if (reset || this._view.following) {
      this._view.updateLines(shown);
      if (reset) this._view.scrollToBottom();
    } else {
      this._pausedNew = Math.max(this._pausedNew, shown.length - this._view.count);
    }
    this._syncFollowUi();
    this._syncCount();
  }

  _syncCount() {
    const el = this.el.querySelector('#logs-count');
    if (el) el.textContent = t('console.linesShown',
      { shown: this._shown.length, total: this._lines.length });
  }

  _syncFollowUi() {
    const following = this._view?.following ?? true;
    this._stickEl?.classList.toggle('show', !following);
    if (this._resumeEl) {
      this._resumeEl.style.display = following ? 'none' : 'inline-flex';
      if (this._newCountEl) {
        this._newCountEl.textContent = this._pausedNew > 0 ? `+${this._pausedNew}` : '';
      }
    }
    const label = this.el.querySelector('#logs-follow');
    if (label) {
      label.textContent = following
        ? t('logs.following')
        : (this._pausedNew > 0
          ? `${t('logs.paused')} · +${this._pausedNew}`
          : t('logs.paused'));
    }
  }

  /** Phân tích lỗi theo blueprint -> chip insights; click chip lọc console. */
  async _analyze() {
    const btn = this.el.querySelector('#console-analyze');
    if (btn) btn.disabled = true;
    const holder = this.el.querySelector('#console-insights');
    try {
      const res = await call('console_analyze',
        this._source === 'launcher' ? this._instanceId : this._instanceId,
        this._source === 'launcher' ? null : this._source);
      if (!res?.ok) return;
      const insights = res.data.insights || [];
      if (!holder) return;
      if (!insights.length) {
        holder.innerHTML = `<div class="empty" style="padding:14px">
          <div>${t('console.noInsights')}</div></div>`;
        return;
      }
      holder.innerHTML = `<div class="chip-row">
        ${insights.map((ins) => `
          <span class="chip ${ins.severity === 'error' ? 'danger' : 'warn'}"
                data-ins="${esc(ins.id)}" role="button" tabindex="0"
                data-tip="${esc(ins.lines?.[0]?.text || '')}">
            ${ins.severity === 'error' ? icon('alert', 12) : icon('info', 12)}
            ${esc(t(`insights.${ins.seed}`))}
            <b>${ins.count}</b>
          </span>`).join('')}
      </div>
      <div class="insight-details" id="insight-details" hidden></div>`;
      holder.querySelectorAll('[data-ins]').forEach((chip) => {
        chip.addEventListener('click', () => this._showInsight(
          insights.find((i) => i.id === chip.dataset.ins)));
      });
    } finally {
      if (btn) btn.disabled = false;
    }
  }

  _showInsight(ins) {
    const box = this.el.querySelector('#insight-details');
    if (!box || !ins) return;
    box.hidden = false;
    const route = ins.action?.kind === 'route' ? ins.action.target : null;
    box.innerHTML = `
      <div class="insight-card">
        <div class="row-between">
          <b>${esc(t(`insights.${ins.seed}`))}</b>
          <span class="muted">${esc(t('console.occurrences', { count: ins.count }))}</span>
        </div>
        <div>${icon('sparkle', 13)} ${esc(t(`insights.${ins.seed}.rec`))}</div>
        ${route ? `<button class="btn sm" data-route="${esc(route)}">
          ${icon('externalLink', 13, 'btn-icon')}${t('console.jumpToRoute')}</button>` : ''}
        ${ins.lines?.length ? `<div class="console vlog" style="max-height:140px;min-height:0">
          ${ins.lines.map((l) => `<div class="vlog-line">${esc(l.text)}</div>`).join('')}
        </div>` : ''}
      </div>`;
    box.querySelector('[data-route]')?.addEventListener('click', () => {
      window.dispatchEvent(new CustomEvent('nav', { detail: route }));
    });
  }

  /** Net check: endpoints launcher (host rỗng) hoặc host:port + MC ping. */
  async _runNetCheck() {
    const out = this.el.querySelector('#net-results');
    if (!out) return;
    const host = this.el.querySelector('#net-host')?.value?.trim() || '';
    const port = parseInt(this.el.querySelector('#net-port')?.value, 10) || 25565;
    out.innerHTML = `<span class="muted">${t('console.analyzing')}</span>`;
    const res = await call('net_check', host || null, port);
    if (!res?.ok) { out.textContent = res?.error?.message || 'error'; return; }
    const data = res.data;

    const rows = [];
    if (data.endpoints) {
      rows.push(`<div class="net-group"><b>${esc(t('console.netEndpoints'))}</b>
        ${data.endpoints.map((e) => `
          <div class="net-row">
            <span>${esc(e.id)} · ${esc(e.host)}:${e.port}</span>
            <span class="${e.ok ? 'ok' : 'err'}">${e.ok
              ? `${t('console.netOk')} · ${t('console.netLatency', { ms: e.ms })}`
              : `${t('console.netFail')} · ${esc(e.error || '')}`}</span>
          </div>`).join('')}</div>`);
    }
    if (data.tcp) {
      rows.push(`<div class="net-row">
        <span>TCP ${esc(data.tcp.host)}:${data.tcp.port}</span>
        <span class="${data.tcp.ok ? 'ok' : 'err'}">${data.tcp.ok
          ? `${t('console.netOk')} · ${t('console.netLatency', { ms: data.tcp.ms })}`
          : `${t('console.netFail')} · ${esc(data.tcp.error || '')}`}</span>
      </div>`);
    }
    if (data.ping) {
      const p = data.ping;
      rows.push(`<div class="net-group"><b>${esc(t('console.netPingTitle'))}</b>
        ${p.online ? `
          <div class="net-row"><span>MOTD</span><span>${esc(p.motd || '—')}</span></div>
          <div class="net-row"><span>${esc(p.version?.name || '?')}</span>
            <span>${esc(t('console.netPlayers', p.players))}</span></div>
          <div class="net-row"><span>${t('console.netLatency', { ms: p.latencyMs })}</span>
            <span class="ok">${t('console.netOk')}</span></div>`
        : `<div class="net-row"><span>${t('console.netOffline')}</span>
            <span class="err">${esc(p.error || '')}</span></div>`}
      </div>`);
    }
    out.innerHTML = rows.join('') || '—';
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._view?.destroy();
    this._view = null;
  }
}

/** Mức log của launcher: "… INFO antares.x message" hoặc meta level có sẵn. */
function classifyLog(text) {
  if (/\b(ERROR|FATAL|CRITICAL)\b/.test(text)) return 'err';
  if (/\bWARNING\b|\bWARN\b/.test(text)) return 'warn';
  if (/\bINFO\b/.test(text)) return 'info';
  return '';
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
