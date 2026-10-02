/**
 * Dashboard tab v2 — trang chủ chỉnh chu (mục 188):
 * Hero instance + hàng stat (số acc/instances/version/net) + panel hệ thống
 * (RAM bar, CPU threads, battery) + 2 biểu đồ live CPU/RAM từ telemetry.
 * Charts chỉ vẽ khi có mẫu mới (event coalesce — mục 79), cleanup khi rời tab.
 */
import { call } from '../../app/bridge.js';
import { t } from '../../i18n/i18n.js';
import { icon } from '../../app/icons.js';
import { navigate } from '../../app/router.js';
import { onBackendEvents, onBackendEvent } from '../../app/events.js';
import { Sparkline } from '../../app/chart.js';

/** Dashboard chỉ là tổng quan: hiển thị vài bản cài gần đây (mục 188). */
const RECENT_LIMIT = 8;
/** Điểm biểu đồ tối đa (~10 phút ở nhịp telemetry thường). */
const CHART_POINTS = 120;

export class DashboardView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._cpuChart = null;
    this._ramChart = null;
    this._cpuHist = [];
    this._ramHist = [];
  }

  async mount() {
    // Remount nội bộ (khi selection đổi): gỡ subscription cũ trước.
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._cpuChart = null;
    this._ramChart = null;
    this._cpuHist = [];
    this._ramHist = [];

    // State + hardware song song — overview chỉ đọc psutil, nhanh.
    const [state, sysRes] = await Promise.all([
      call('app_get_state'),
      call('system_overview'),
    ]);
    const data = state?.ok ? state.data : {};
    const sys = sysRes?.ok ? sysRes.data : {};
    const instances = data.instancesSummary || [];
    const accounts = data.accountsSummary || [];
    const selectedId = data.selectedInstance;
    const selected = instances.find((i) => i.id === selectedId) || instances[0] || null;
    const account = accounts.find((a) => a.id === data.selectedAccount) || accounts[0] || null;
    const online = navigator.onLine !== false;
    const hw = sys.hardware || {};
    const ramTotalGb = hw.ramTotalMb ? (hw.ramTotalMb / 1024) : null;
    const ramPercent = typeof hw.ramPercent === 'number' ? hw.ramPercent : null;
    const ramUsedGb = (ramTotalGb != null && typeof hw.ramUsedMb === 'number')
      ? hw.ramUsedMb / 1024 : null;

    this.el.innerHTML = `
      <div class="view-title">${t('dashboard.title')}</div>
      <div class="view-sub">${t('dashboard.subtitle')}</div>

      <div class="hero ${selected ? 'has-art' : ''}" ${selected ? heroArtStyle(selected) : ''}>
        <div class="hero-kicker">${icon('star', 13)} ${t('dashboard.selectedInstance')}</div>
        ${selected ? `
          <div class="hero-name">${esc(selected.name)}</div>
          <div class="hero-meta">
            <span class="badge accent">${esc(selected.loader || 'vanilla')}</span>
            <span class="badge">${esc(selected.version || '?')}</span>
            <span class="badge">${esc(account?.displayName || t('dashboard.none'))}</span>
            <span class="badge runtime-fps" id="dash-fps" hidden>
              ${icon('diagnostics', 12)} <span>—</span>
            </span>
          </div>
          <div class="hero-actions">
            <button class="btn primary btn-play" id="dash-play">${icon('play', 16, 'btn-icon')}${t('dashboard.play')}</button>
            <button class="btn ghost" id="dash-instances">${icon('instances', 15, 'btn-icon')}${t('dashboard.instances')}</button>
          </div>`
        : `
          <div class="hero-name">${t('dashboard.noInstance')}</div>
          <div class="hero-actions">
            <button class="btn primary" id="dash-new">${icon('plus', 15, 'btn-icon')}${t('dashboard.createFirst')}</button>
          </div>`}
      </div>

      <div class="grid-4 section stagger">
        ${statTile('users', t('dash.accounts'), String(accounts.length), {
          tone: 'accent', sub: account?.displayName || t('dashboard.none'),
          onClick: 'accounts',
        })}
        ${statTile('instances', t('dash.instances'), String(instances.length), {
          sub: selected?.name || t('dashboard.none'),
          onClick: 'instances',
        })}
        ${statTile('cube', t('dash.version'), selected?.version || t('dashboard.none'), {
          sub: selected?.loader || 'vanilla',
        })}
        ${statTile(online ? 'wifi' : 'xCircle', t('dash.network'),
          online ? t('topbar.online') : t('topbar.offline'), {
          tone: online ? 'ok' : 'danger',
          onClick: 'logs',
        })}
      </div>

      <div class="section sys-panel card" id="dash-sys">
        <div class="sys-head">
          <h3>${icon('monitor', 14)} ${t('dash.system')}</h3>
          <span class="badge info live-dot" id="dash-game-mode" hidden>${t('dash.gameMode')}</span>
        </div>
        <div class="sys-grid">
          <div class="sys-block">
            <div class="kv-row">
              <span class="kv-label">${icon('memory', 13)} ${t('dash.ram')}</span>
              <span class="kv-val" id="dash-ram-text">${ramPercent != null
                ? `${ramPercent}%` : '—'}</span>
            </div>
            <div class="meter" id="dash-ram-meter" title="${ramTotalGb
              ? t('dash.ramTotal', { gb: ramTotalGb.toFixed(1) }) : ''}">
              <div style="width:${ramPercent ?? 0}%"></div>
            </div>
            <div class="sys-sub">${ramUsedGb != null
              ? t('dash.ramUsage', { used: ramUsedGb.toFixed(1), total: ramTotalGb.toFixed(1) })
              : t('dash.sysUnavailable')}</div>
            <div class="sys-chart-wrap"><canvas id="dash-chart-ram" height="56"></canvas></div>
          </div>
          <div class="sys-block">
            <div class="kv-row">
              <span class="kv-label">${icon('cpu', 13)} ${t('dash.cpu')}</span>
              <span class="kv-val" id="dash-cpu-text">—</span>
            </div>
            <div class="meter accent" id="dash-cpu-meter"><div style="width:0%"></div></div>
            <div class="sys-sub">${hw.cpuThreads
              ? t('dash.cpuThreads', { n: hw.cpuThreads })
              : t('dash.sysUnavailable')}</div>
            <div class="sys-chart-wrap"><canvas id="dash-chart-cpu" height="56"></canvas></div>
          </div>
          <div class="sys-block">
            <div class="kv-row">
              <span class="kv-label">${icon('activity', 13)} ${t('dash.launcher')}</span>
              <span class="kv-val" id="dash-launcher-text">—</span>
            </div>
            <div class="meter" id="dash-launcher-meter"><div style="width:0%"></div></div>
            <div class="sys-sub" id="dash-battery">${batteryText(hw.battery)}</div>
          </div>
        </div>
      </div>

      <div class="section">
        <div class="row-between" style="margin-bottom:12px">
          <h3 style="margin:0">${t('dashboard.instances')}</h3>
          ${instances.length > RECENT_LIMIT
            ? `<button class="btn ghost sm" id="dash-all">${t('dashboard.viewAll', { count: instances.length })}</button>`
            : ''}
        </div>
        ${instances.length === 0
          ? `<div class="card"><div class="empty">
               <div class="big">${icon('instances', 26)}</div>
               <div class="empty-title">${t('dashboard.noInstances')}</div>
               <button class="btn primary" id="dash-new2">${icon('plus', 15, 'btn-icon')}${t('dashboard.createFirst')}</button>
             </div></div>`
          : `<div class="grid-auto stagger">
              ${instances.slice(0, RECENT_LIMIT).map((i) => `
                <div class="inst-card ${i.id === selected?.id ? 'selected' : ''}" data-select="${esc(i.id)}" role="button" tabindex="0">
                  <div class="inst-top">
                    <span class="inst-mark">${icon('instances', 18)}</span>
                    <span class="grow inst-name truncate">${esc(i.name)}</span>
                  </div>
                  <div class="inst-foot">
                    <span class="badge accent">${esc(i.loader || 'vanilla')}</span>
                    <span class="badge">${esc(i.version || '?')}</span>
                  </div>
                </div>`).join('')}
            </div>`}
      </div>`;

    this._wire(selected);
    this._initCharts();
  }

  _wire(selected) {
    this.el.querySelector('#dash-play')?.addEventListener('click', async () => {
      if (!selected) return;
      await call('instances_select', selected.id);
      navigate('play');
    });
    const go = (route) => navigate(route);
    this.el.querySelector('#dash-instances')?.addEventListener('click', () => go('instances'));
    this.el.querySelector('#dash-new')?.addEventListener('click', () => go('instances'));
    this.el.querySelector('#dash-new2')?.addEventListener('click', () => go('instances'));
    this.el.querySelector('#dash-all')?.addEventListener('click', () => go('instances'));
    // Stat tile có đích -> click nhảy tab (UIX: dashboard là hub điều hướng)
    this.el.querySelectorAll('[data-route]').forEach((tile) => {
      tile.addEventListener('click', () => go(tile.dataset.route));
      tile.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); go(tile.dataset.route); }
      });
    });

    this.el.querySelectorAll('[data-select]').forEach((card) => {
      const pick = async () => {
        await call('instances_select', card.dataset.select);
        this.mount();
      };
      card.addEventListener('click', pick);
      card.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); pick(); }
      });
    });

    this._unsubs.push(
      onBackendEvents(['selection.changed', 'accounts.changed', 'instances.changed'], () => {
        if (!this.el?.isConnected) return;
        this.mount();
      }),
      // FPS từ companion mod -> badge LIVE trên hero (mục 12; event đã coalesce)
      onBackendEvent('runtime.performance', (env) => {
        const fps = env?.payload?.fps;
        const badge = this.el.querySelector('#dash-fps');
        if (!badge) return;
        if (typeof fps === 'number' && fps > 0) {
          badge.hidden = false;
          badge.querySelector('span').textContent = `${Math.round(fps)} FPS`;
        } else {
          badge.hidden = true;
        }
      }),
      // Telemetry live: biểu đồ + meter (event coalesce latest — mục 79)
      onBackendEvent('performance.telemetry', (env) => this._onTelemetry(env?.payload)),
    );
  }

  _initCharts() {
    const cpuCanvas = this.el.querySelector('#dash-chart-cpu');
    const ramCanvas = this.el.querySelector('#dash-chart-ram');
    if (cpuCanvas) {
      this._cpuChart = new Sparkline(cpuCanvas, { max: 100, min: 0 });
    }
    if (ramCanvas) {
      this._ramChart = new Sparkline(ramCanvas, { max: 100, min: 0 });
    }
    // Mồi biểu đồ từ snapshot hiện có (Performance sampler chạy từ lúc boot).
    call('performance_snapshot').then((res) => {
      if (!res?.ok || !this.el?.isConnected) return;
      const hist = res.data.history || [];
      const latest = res.data.latest;
      for (const s of hist.slice(-CHART_POINTS)) this._pushPoint(s, true);
      if (latest) this._onTelemetry(latest, true);
    }).catch(() => {});
  }

  _onTelemetry(sample, silent = false) {
    if (!sample || !this.el?.isConnected) return;
    this._pushPoint(sample, silent);

    const cpu = typeof sample.cpu === 'number' ? sample.cpu : null;
    const ramPct = sample.ram?.percent;
    const launcher = sample.launcher;
    const cpuMeter = this.el.querySelector('#dash-cpu-meter div');
    const cpuText = this.el.querySelector('#dash-cpu-text');
    if (cpu != null) {
      if (cpuMeter) cpuMeter.style.width = `${Math.min(100, cpu)}%`;
      if (cpuText) cpuText.textContent = `${cpu.toFixed(0)}%`;
    }
    const ramText = this.el.querySelector('#dash-ram-text');
    const ramMeter = this.el.querySelector('#dash-ram-meter div');
    if (typeof ramPct === 'number') {
      if (ramText) ramText.textContent = `${ramPct}%`;
      if (ramMeter) ramMeter.style.width = `${Math.min(100, ramPct)}%`;
    }
    if (launcher && typeof launcher.cpu === 'number') {
      const text = this.el.querySelector('#dash-launcher-text');
      const meter = this.el.querySelector('#dash-launcher-meter div');
      if (text) text.textContent = `${launcher.cpu.toFixed(1)}%`;
      if (meter) meter.style.width = `${Math.min(100, launcher.cpu)}%`;
    }
    const gm = this.el.querySelector('#dash-game-mode');
    if (gm) gm.hidden = !sample.gameMode;
  }

  _pushPoint(sample, silent) {
    const ts = sample.ts || Date.now() / 1000;
    if (typeof sample.cpu === 'number') {
      this._cpuHist.push([ts, sample.cpu]);
      if (this._cpuHist.length > CHART_POINTS) this._cpuHist.shift();
      this._cpuChart?.setData(this._cpuHist);
    }
    if (typeof sample.ram?.percent === 'number') {
      this._ramHist.push([ts, sample.ram.percent]);
      if (this._ramHist.length > CHART_POINTS) this._ramHist.shift();
      this._ramChart?.setData(this._ramHist);
    }
    if (silent) return;
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._cpuChart = null;   // rời tab = ngừng vẽ (mục 78)
    this._ramChart = null;
  }
}

/** Text pin: "85% (sạc)" / "42%" / gợi ý nếu không có dữ liệu. */
function batteryText(batt) {
  if (!batt || typeof batt.percent !== 'number') return t('dash.noBattery');
  return `${Math.round(batt.percent)}%${batt.plugged ? ` · ${t('dash.charging')}` : ''}`;
}

function statTile(iconName, label, value, { tone = '', sub = '', onClick } = {}) {
  return `
    <div class="stat ${tone} ${onClick ? 'clickable' : ''}" ${onClick ? `data-route="${onClick}" role="button" tabindex="0"` : ''}>
      <span class="stat-icon">${icon(iconName, 16)}</span>
      <span class="stat-label">${esc(label)}</span>
      <span class="stat-value truncate">${esc(value)}</span>
      ${sub ? `<span class="stat-sub truncate">${esc(sub)}</span>` : ''}
      ${onClick ? `<span class="stat-go">${icon('arrowRight', 12)}</span>` : ''}
    </div>`;
}

/** Art hero: gradient 2 màu sinh từ hash tên instance — mỗi instance 1 màu
 *  riêng, không cần asset (bài học Modrinth/CurseForge — UIUX-UPGRADE §2.4). */
function heroArtStyle(instance) {
  let h = 0;
  const s = String(instance.name || instance.id || '');
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  const hue1 = h % 360;
  const hue2 = (h % 360 + 46) % 360;   // lệch 46° — cùng tông, phân biệt rõ
  return `style="--art-h1:${hue1};--art-h2:${hue2}"`;
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
