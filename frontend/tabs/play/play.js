/**
 * Play tab — Play Center (wireframe 189, state machine 280).
 * Play button chỉ gọi store->api — không gọi launch thẳng (mục 279).
 */
import { call } from '../../app/bridge.js';
import { toast, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { createVirtualLog } from '../../app/virtual-log.js';
import { onBackendEvent } from '../../app/events.js';
import * as runtime from '../../state/runtime-store.js';
import { Sparkline } from '../../app/chart.js';

export class PlayView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._taskId = null;
    this._expect = null;   // {type, owner} của task đang chờ nhận
    this._selected = null;
    this._account = null;
    this._view = null;
    this._jumpEl = null;
    this._log = [];
  }

  async mount() {
    this._detach();
    const [state, instRes] = await Promise.all([call('app_get_state'), call('instances_list')]);
    const data = state?.ok ? state.data : {};
    const instances = instRes?.ok ? (instRes.data.instances || []) : (data.instancesSummary || []);
    const accounts = data.accountsSummary || [];
    const selected = instances.find((i) => i.id === data.selectedInstance) || instances[0] || null;
    const account = accounts.find((a) => a.id === data.selectedAccount) || accounts[0] || null;
    this._selected = selected;
    this._account = account;

    this.el.innerHTML = `
      <div class="view-title">${t('play.title')}</div>
      <div class="view-sub">${t('play.subtitle')}</div>

      <div class="play-card">
        <div class="grid-2 stagger">
          <div class="field">
            <label for="play-account">${t('play.account')}</label>
            <select id="play-account">
              ${accounts.length === 0 ? `<option value="">${t('play.noAccount')}</option>` : ''}
              ${accounts.map((a) => `<option value="${esc(a.id)}" ${a === account ? 'selected' : ''}>
                ${esc(a.displayName)} (${esc(a.type)})</option>`).join('')}
            </select>
          </div>
          <div class="field">
            <label for="play-instance">${t('play.instance')}</label>
            <select id="play-instance">
              ${instances.length === 0 ? `<option value="">${t('play.noInstance')}</option>` : ''}
              ${instances.map((i) => `<option value="${esc(i.id)}" ${i === selected ? 'selected' : ''}>
                ${esc(i.name)}</option>`).join('')}
            </select>
          </div>
        </div>

        ${selected ? `
          <div class="card static" style="background:transparent;border-color:var(--border-0)">
            <div class="kv"><span class="kv-key">${icon('layers', 13)} ${t('play.loader')}</span>
              <span class="kv-val"><span class="badge accent">${esc(selected.loader || 'vanilla')}</span></span></div>
            <div class="kv"><span class="kv-key">${icon('instances', 13)} ${t('play.minecraft')}</span>
              <span class="kv-val">${esc(selected.minecraftVersion || selected.version || '?')}</span></div>
            <div class="kv"><span class="kv-key">${icon('cpu', 13)} ${t('play.java')}</span>
              <span class="kv-val">${esc(selected.java?.mode || 'auto')}</span></div>
            <div class="kv"><span class="kv-key">${icon('memory', 13)} ${t('play.memory')}</span>
              <span class="kv-val">${selected.memory?.maxMb || 2048} MB</span></div>
          </div>` : ''}

        <div class="play-orb">
          <button class="btn play primary" id="play-btn" ${!selected ? 'disabled' : ''}>
            ${t('play.launch')}
          </button>
        </div>
        <div class="play-status" id="play-status">${t('play.ready')}</div>
        <div class="progress" id="play-progress" style="display:none"><div style="width:0%"></div></div>
        <div class="center" style="margin-top:12px">
          <button class="btn ghost hidden" id="play-cancel">${icon('close', 14, 'btn-icon')}${t('play.cancel')}</button>
        </div>
      </div>

      <div class="card section" id="play-runtime" hidden>
        <h3>${icon('rocket', 14)} ${t('runtime.title')} <span class="badge accent dot live" id="runtime-live">${t('runtime.live')}</span></h3>
        <div class="runtime-stats">
          <div class="runtime-stat"><span class="runtime-stat-value" id="runtime-fps">—</span><small>FPS</small></div>
          <div class="runtime-stat"><span class="runtime-stat-value" id="runtime-frame">—</span><small>ms</small></div>
          <div class="runtime-stat"><span class="runtime-stat-value" id="runtime-low1">—</span><small>1% low</small></div>
          <canvas id="runtime-fps-chart" class="runtime-spark" width="280" height="48"></canvas>
        </div>
      </div>

      <div class="card section">
        <h3>${icon('logs', 14)} ${t('play.console')}</h3>
        <div class="console-wrap">
          <div class="console vlog" id="play-console" style="height:260px"></div>
          <button class="console-jump" id="play-jump" type="button">
            ${icon('chevronDown', 14)}${esc(t('logs.jumpToBottom'))}
          </button>
        </div>
      </div>`;

    // Console ảo hoá cho output khởi chạy (mục 16)
    this._view?.destroy();
    this._log = [];
    const consoleBox = this.el.querySelector('#play-console');
    this._view = createVirtualLog({ container: consoleBox, emptyText: t('play.consoleHint') });
    this._jumpEl = this.el.querySelector('#play-jump');
    consoleBox.addEventListener('scroll', () => this._syncJump(), { passive: true });
    this._jumpEl?.addEventListener('click', () => {
      this._view.scrollToBottom();
      this._syncJump();
    });

    // Runtime companion panel (FPS/frametime khi companion mod kết nối — mục 12)
    this._initRuntimePanel();

    this.el.querySelector('#play-account')?.addEventListener('change', (e) => {
      this._account = accounts.find((a) => a.id === e.target.value) || null;
    });
    this.el.querySelector('#play-instance')?.addEventListener('change', async (e) => {
      const next = instances.find((i) => i.id === e.target.value) || null;
      this._selected = next;
      if (next) await call('instances_select', next.id);
      this.mount();
    });
    this.el.querySelector('#play-btn')?.addEventListener('click', () => this._launch());
    this.el.querySelector('#play-cancel')?.addEventListener('click', () => this._cancel());

    // Tiến trình đến từ event đẩy — không còn poll tasks_list mỗi 800ms (mục 15.2)
    this._unsubs.push(
      onBackendEvent('task.updated', (e) => this._onTaskEvent(e.payload)),
      onBackendEvent('minecraft.*', (e) => this._onMinecraftEvent(e)),
    );
  }

  _detach() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._taskId = null;
    this._expect = null;
  }

  _onMinecraftEvent(entry) {
    const payload = entry.payload || {};
    if (Array.isArray(payload.lines)) payload.lines.forEach((l) => this._pushLog(l));
    else if (payload.line) this._pushLog(payload.line);
    if (payload.version && entry.event === 'minecraft.launching') {
      this._pushLog(`${t('play.validating')} ${payload.version}`);
    }
    if (entry.event === 'minecraft.exited') {
      const btn = this.el.querySelector('#play-btn');
      const status = this.el.querySelector('#play-status');
      if (btn) { btn.disabled = false; btn.textContent = t('play.launch'); }
      if (status) { status.className = 'play-status'; status.textContent = t('play.ready'); }
    }
    if (entry.event === 'minecraft.error') {
      handleError(payload.error || payload);
    }
  }

  async _launch() {
    const instance = this._selected;
    const account = this._account;
    if (!instance) return;
    if (!account) { toastError(t('play.error.noAccount')); return; }

    await call('accounts_select', account.id);
    await call('instances_select', instance.id);

    const btn = this.el.querySelector('#play-btn');
    const status = this.el.querySelector('#play-status');
    btn.disabled = true;
    btn.textContent = t('play.checking');
    status.className = 'play-status busy';
    status.textContent = t('play.validating');

    this._expect = { type: 'LAUNCH', owner: `instance:${instance.id}` };
    const res = await call('minecraft_launch', instance.id);
    if (!res.ok) {
      this._expect = null;
      handleError(res.error);
      btn.disabled = false;
      btn.textContent = t('play.launch');
      status.className = 'play-status error';
      status.textContent = res.error?.code || t('common.error');
      return;
    }
    this._taskId = res.data.taskId;
    btn.textContent = t('play.starting');
    this.el.querySelector('#play-cancel')?.classList.remove('hidden');
    this._pushLog(`${t('play.starting')} · ${instance.name}`);
    toast(t('play.toast.started'), 'info');
  }

  /** Ghi 1 dòng vào console ảo (bỏ qua dòng lặp liên tiếp). */
  _pushLog(text) {
    if (!text) return;
    const line = String(text);
    if (this._log[this._log.length - 1] === line) return;
    this._log.push(line);
    this._view?.updateLines(this._log);
    this._syncJump();
  }

  _syncJump() {
    this._jumpEl?.classList.toggle('show', !(this._view?.following ?? true));
  }

  async _cancel() {
    if (!this._taskId) return;
    await call('tasks_cancel', this._taskId);
    this._taskId = null;
    const btn = this.el.querySelector('#play-btn');
    const status = this.el.querySelector('#play-status');
    if (btn) { btn.disabled = false; btn.textContent = t('play.launch'); }
    if (status) { status.className = 'play-status'; status.textContent = t('play.cancelled'); }
    this.el.querySelector('#play-cancel')?.classList.add('hidden');
    this.el.querySelector('#play-progress')?.style.setProperty('display', 'none');
  }

  /**
   * Task của lần khởi chạy này (event `task.updated`).
   * Nhận theo id, hoặc theo (type, owner) khi event tới trước lúc lệnh trả về.
   */
  _onTaskEvent(task) {
    if (!task) return;
    const known = this._taskId && task.id === this._taskId;
    const expected = this._expect
      && task.type === this._expect.type && task.owner === this._expect.owner;
    if (!known && !expected) return;
    this._taskId = task.id;

    const bar = this.el.querySelector('#play-progress');
    const fill = bar?.querySelector('div');
    const status = this.el.querySelector('#play-status');
    const btn = this.el.querySelector('#play-btn');
    if (bar) bar.style.display = 'block';
    if (fill) fill.style.width = `${task.progress || 0}%`;
    if (status) {
      status.className = 'play-status busy';
      status.textContent = task.message || task.state;
    }

    this._pushLog(task.message);

    if (task.state === 'completed') {
      this._taskId = null;
      this._expect = null;
      if (btn) btn.textContent = t('play.running');
      if (status) { status.className = 'play-status'; status.textContent = t('play.running'); }
      this.el.querySelector('#play-cancel')?.classList.add('hidden');
    } else if (task.state === 'failed') {
      this._taskId = null;
      this._expect = null;
      if (btn) { btn.disabled = false; btn.textContent = t('play.retry'); }
      this.el.querySelector('#play-cancel')?.classList.add('hidden');
      handleError(task.error);           // launch error có action one-click
    } else if (task.state === 'cancelled') {
      this._taskId = null;
      this._expect = null;
      if (btn) { btn.disabled = false; btn.textContent = t('play.launch'); }
      this.el.querySelector('#play-cancel')?.classList.add('hidden');
      if (status) { status.className = 'play-status'; status.textContent = t('play.cancelled'); }
    }
  }

  unmount() {
    this._detach();
    this._view?.destroy();
    this._view = null;
    if (this._runtimeOff) { this._runtimeOff(); this._runtimeOff = null; }
    this._fpsChart = null; // rời tab = ngừng vẽ (mục 78)
  }

  /* ---------------- Runtime companion panel (mục 5, 12) ---------------- */

  _initRuntimePanel() {
    runtime.init();
    this._runtimeOff = runtime.onChange((s) => this._renderRuntime(s));
    this._renderRuntime(runtime.getState());
  }

  _renderRuntime(s) {
    if (!this.el.isConnected) return;
    const panel = this.el.querySelector('#play-runtime');
    if (!panel) return;
    panel.hidden = !s.connected;
    if (!s.connected) return;

    const fpsEl = this.el.querySelector('#runtime-fps');
    const frameEl = this.el.querySelector('#runtime-frame');
    const low1El = this.el.querySelector('#runtime-low1');
    if (fpsEl) fpsEl.textContent = s.latest?.fps != null ? String(Math.round(s.latest.fps)) : '—';
    if (frameEl) frameEl.textContent = s.latest?.frameMs != null ? s.latest.frameMs.toFixed(1) : '—';
    if (low1El) low1El.textContent = s.latest?.low1 != null ? String(Math.round(s.latest.low1)) : '—';

    // Sparkline chỉ vẽ khi tab đang mount + có data (mục 78/79)
    if (!this._fpsChart) {
      const canvas = this.el.querySelector('#runtime-fps-chart');
      if (canvas) this._fpsChart = new Sparkline(canvas, { color: '#35e08f' });
    }
    if (s.fpsHistory.length) {
      this._fpsChart?.setData(s.fpsHistory.slice(-60));
    }
  }
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
