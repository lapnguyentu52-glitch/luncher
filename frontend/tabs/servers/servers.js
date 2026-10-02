/** Servers tab — list + create + console (mục 20, 148, 193-194). */
import { call } from '../../app/bridge.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { openModal, confirmDialog } from '../../app/modal.js';
import { createVirtualLog } from '../../app/virtual-log.js';
import { onBackendEvents } from '../../app/events.js';

const SOFTWARE = ['vanilla', 'paper', 'purpur', 'fabric', 'velocity', 'custom'];
const LEVELS = ['all', 'info', 'warn', 'error'];

const CONSOLE_MAX_LINES = 2000;

export class ServersView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._view = null;
    this._jumpEl = null;
    this._resumeEl = null;
    this._newCountEl = null;
    this._selectedId = null;    // server đang hiển thị trong console
    this._servers = [];         // list từ backend (đã kèm running)
    // Console theo từng server: { [serverId]: string[] } — không lẫn dòng khi
    // nhiều server cùng chạy (event payload luôn có serverId ở meta).
    this._consoles = new Map();
    this._filter = '';        // từ khoá lọc console (như Logs tab)
    this._level = 'all';      // all | info | warn | error
    this._reloadTimer = null;
    this._metrics = new Map();  // { [serverId]: {cpuPercent, memoryMb, ...} }
    this._pausedNew = 0;        // số dòng mới tới khi user đang cuộn lên đọc
  }

  async mount() {
    // Remount nội bộ (sau start/stop/create/delete): gỡ subscription cũ trước
    // để không nhận event trùng -> console không bị lặp dòng.
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    const servers = res?.ok ? res.data.servers : [];
    this._servers = servers;
    if (!this._selectedId && servers.length) this._selectedId = servers[0].id;

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('servers.title')}</div>
          <div class="view-sub">${t('servers.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn primary" id="srv-new">${icon('plus', 15, 'btn-icon')}${t('servers.create')}</button>
        </div>
      </div>

      <div class="card">
        ${servers.length === 0
          ? `<div class="empty">
               <div class="big">${icon('servers', 26)}</div>
               <div class="empty-title">${t('servers.none')}</div>
             </div>`
          : servers.map((s) => this._serverRow(s)).join('')}
      </div>

      <div class="card section">
        <h3>${icon('logs', 14)} ${t('servers.console')}</h3>
        ${servers.length > 1 ? `
        <div class="toolbar" style="margin-bottom:10px">
          <select id="srv-console-server" class="grow">
            ${servers.map((s) => `<option value="${esc(s.id)}" ${s.id === this._selectedId ? 'selected' : ''}>
              ${esc(s.name)}${s.running ? ' ●' : ''}</option>`).join('')}
          </select>
          <span class="muted" id="srv-metrics"></span>
        </div>` : ''}
        <div class="toolbar" style="margin-bottom:10px">
          <input type="text" id="srv-filter" class="grow" placeholder="${esc(t('logs.filter'))}" />
          <div class="seg" id="srv-levels">
            ${LEVELS.map((l) => `<button data-level="${l}" class="${l === 'all' ? 'active' : ''}">
              ${l === 'all' ? esc(t('common.all')) : l}</button>`).join('')}
          </div>
        </div>
        <div class="console-wrap">
          <div class="console vlog" id="srv-console" style="height:240px"></div>
          <button class="console-jump" id="srv-jump" type="button">
            ${icon('chevronDown', 14)}${esc(t('logs.jumpToBottom'))}
          </button>
          <button class="console-jump" id="srv-resume" type="button" style="display:none">
            ${icon('chevronDown', 14)}${esc(t('servers.resumeFollow'))}<span class="muted" id="srv-newcount"></span>
          </button>
          <button class="console-clear" id="srv-clear" type="button"
                  title="${esc(t('servers.clearConsole'))}" aria-label="${esc(t('servers.clearConsole'))}">
            ${icon('eraser', 14)}${esc(t('servers.clearConsole'))}
          </button>
        </div>
        <div class="input-group" style="margin-top:10px">
          <input type="text" id="srv-cmd" placeholder="${esc(t('servers.commandPlaceholder'))}" />
          <button class="btn" id="srv-send">${icon('play', 14, 'btn-icon')}${t('servers.send')}</button>
        </div>
      </div>`;

    // Console ảo hoá: dựng lại mỗi lần mount (dọn view cũ trước — mục 16)
    this._view?.destroy();
    const consoleBox = this.el.querySelector('#srv-console');
    this._view = createVirtualLog({
      container: consoleBox,
      emptyText: '—',
      classify: classifyServerLine,   // tô màu INFO/WARN/ERROR/Chat (mục 16)
    });
    this._jumpEl = this.el.querySelector('#srv-jump');
    this._resumeEl = this.el.querySelector('#srv-resume');
    this._newCountEl = this.el.querySelector('#srv-newcount');
    consoleBox.addEventListener('scroll', () => this._syncJump(), { passive: true });
    this._jumpEl?.addEventListener('click', () => {
      this._view.scrollToBottom();
      this._syncJump();
    });
    // Clear console: xoá buffer server ĐANG XEM (các server khác giữ nguyên)
    this.el.querySelector('#srv-clear')?.addEventListener('click', () => {
      this._consoles.delete(this._selectedId);
      this._view.clear();
      this._pausedNew = 0;
      this._syncJump();
    });
    // Resume follow: về đáy + reset bộ đếm dòng mới
    this._resumeEl?.addEventListener('click', () => {
      this._pausedNew = 0;
      this._view.scrollToBottom();
      this._syncJump();
    });
    this.el.querySelector('#srv-console-server')?.addEventListener('change', (e) => {
      this._selectedId = e.target.value;
      this._renderConsole();
    });
    // Filter + level cho console (như Logs tab) + highlight từ khoá
    this.el.querySelector('#srv-filter')?.addEventListener('input', (e) => {
      this._filter = e.target.value.toLowerCase();
      this._view?.setHighlight(this._filter);
      this._renderConsole();
    });
    this.el.querySelector('#srv-levels')?.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-level]');
      if (!btn) return;
      this._level = btn.dataset.level;
      this.el.querySelectorAll('#srv-levels [data-level]').forEach((x) =>
        x.classList.toggle('active', x === btn));
      this._renderConsole();
    });

    // Console KHÔNG seed từ logs_recent — đó là log launcher, không phải console
    // server. Lịch sử server nằm trong buffer _consoles (giữ qua remount) và
    // các dòng mới tới realtime qua event server.output.

    this.el.querySelector('#srv-new')?.addEventListener('click', () => this._dialog());
    this.el.querySelectorAll('[data-start]').forEach((b) =>
      b.addEventListener('click', () => this._start(b.dataset.start)));
    this.el.querySelectorAll('[data-stop]').forEach((b) =>
      b.addEventListener('click', async () => {
        const r = await call('servers_stop', b.dataset.stop);
        if (!r.ok) { handleError(r.error); return; }
        this.mount();
      }));
    this.el.querySelectorAll('[data-del]').forEach((b) =>
      b.addEventListener('click', async () => {
        const ok = await confirmDialog({
          title: t('common.delete'),
          message: t('servers.confirmDelete'),
          confirmText: t('common.delete'),
        });
        if (!ok) return;
        const r = await call('servers_delete', b.dataset.del, true);
        if (!r.ok) { handleError(r.error); return; }
        toastSuccess(t('servers.toast.deleted'));
        this.mount();
      }));

    this.el.querySelector('#srv-send')?.addEventListener('click', () => this._send());
    this.el.querySelector('#srv-cmd')?.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this._send();
    });

    // Console + trạng thái server được đẩy từ backend — không poll (mục 15.2)
    this._unsubs.push(
      // EventBridge gom server.output thành batch payload.lines; meta kèm serverId
      // của server CUỐI trong batch (gom theo tên event, không theo server) —
      // đủ chính xác cho console tách kênh vì meta cập nhật theo từng publish.
      onBackendEvents(['server.output'], (e) => {
        const payload = e.payload || {};
        const batch = Array.isArray(payload.lines)
          ? payload.lines
          : (payload.line ? [payload.line] : []);
        if (!batch.length) return;
        const id = payload.serverId || this._selectedId;
        this._bufferLines(id, batch);
        if (id === this._selectedId) {
          // Đang cuộn lên đọc -> KHÔNG cập nhật DOM (tránh giật vị trí cuộn),
          // chỉ đếm dòng mới; bấm resume/jump-to-bottom để bắt kịp.
          if (this._view.following) {
            this._view.updateLines(this._applyFilter(this._consoles.get(id) || []));
          } else {
            this._pausedNew += batch.length;
          }
          this._syncJump();
        }
      }),
      // Metrics RAM/CPU realtime (mục 22) — EventBridge chỉ giữ bản mới nhất.
      onBackendEvents(['server.metrics'], (e) => {
        const id = e.payload?.serverId;
        if (!id) return;
        this._metrics.set(id, e.payload || {});
        if (id === this._selectedId) this._renderMetrics();
      }),
      onBackendEvents(['server.started', 'server.stopped', 'server.failed'], (e) => {
        const id = e.payload?.serverId;
        if (!id) return;
        this._reloadListSoon();
        if (e.event === 'server.failed') {
          toastError(t('servers.toast.startFailed', { code: e.payload.exitCode ?? '?' }));
        }
      }),
      onBackendEvents(['servers.changed'], () => this._reloadListSoon()),
    );

    this._renderConsole();
  }

  /** Giữ buffer console THEO SERVER để dòng từ event nối tiếp đúng (append-only). */
  _bufferLines(serverId, batch) {
    const buf = (this._consoles.get(serverId) || []).concat(batch);
    this._consoles.set(serverId,
      buf.length > CONSOLE_MAX_LINES ? buf.slice(buf.length - CONSOLE_MAX_LINES) : buf);
  }

  /** Áp filter + level rồi vẽ console của server đang xem (như Logs tab). */
  _renderConsole() {
    const shown = this._applyFilter(this._consoles.get(this._selectedId) || []);
    this._view?.updateLines(shown);
    this._renderMetrics();
    this._syncJump();
  }

  _applyFilter(lines) {
    let shown = lines;
    if (this._level !== 'all') {
      // Dùng cùng quy tắc với classifyServerLine để filter khớp màu hiển thị
      const re = this._level === 'error'
        ? /\b(ERROR|FATAL|CRITICAL)\b|Exception/
        : new RegExp(`\\b${this._level.toUpperCase()}\\b`);
      shown = shown.filter((l) => re.test(l));
    }
    if (this._filter) shown = shown.filter((l) => l.toLowerCase().includes(this._filter));
    return shown;
  }

  _renderMetrics() {
    const el = this.el.querySelector('#srv-metrics');
    if (!el) return;
    const m = this._metrics.get(this._selectedId);
    if (!m || m.memoryMb == null) { el.textContent = ''; return; }
    el.textContent = t('servers.metrics', {
      mem: m.memoryMb ?? '?',
      cpu: m.cpuPercent != null ? m.cpuPercent : '—',
    });
  }

  _serverRow(s) {
    const running = Boolean(s.running);   // state thật từ backend, không phải state UI
    const selected = s.id === this._selectedId;
    return `
      <div class="row srv-row ${selected ? 'selected' : ''}" data-srv="${esc(s.id)}"
           role="button" tabindex="0" title="${esc(t('servers.viewConsole'))}">
        <span class="badge ${running ? 'online dot' : 'offline dot'}">
          ${running ? t('servers.running') : t('servers.stopped')}</span>
        <span class="grow">
          <div>${esc(s.name)}${selected ? ` <span class="muted">· ${esc(t('servers.viewing'))}</span>` : ''}</div>
          <div class="muted">${esc(s.software)} ${esc(s.version || '')} · ${t('servers.port', { port: s.port })}</div>
        </span>
        ${running
          ? `<button class="btn danger sm" data-stop="${esc(s.id)}">${icon('close', 14, 'btn-icon')}${t('servers.stop')}</button>`
          : `<button class="btn success sm" data-start="${esc(s.id)}">${icon('play', 14, 'btn-icon')}${t('servers.start')}</button>`}
        <button class="btn danger-ghost sm" data-del="${esc(s.id)}" aria-label="${esc(t('common.delete'))}" data-tip="${esc(t('common.delete'))}">
          ${icon('trash', 15)}
        </button>
      </div>`;
  }

  /** Bấm hàng (hoặc Enter/Space) -> đổi console sang server đó ngay. */
  _selectServer(id) {
    if (!id || id === this._selectedId) return;
    this._selectedId = id;
    // Đồng bộ dropdown nếu đang hiển thị
    const select = this.el.querySelector('#srv-console-server');
    if (select) select.value = id;
    // Highlight list + nhãn "đang xem" — đổi class/cục bộ, không remount
    // (remount sẽ destroy/re-create virtual log, gây giật vô ích).
    this.el.querySelectorAll('[data-srv]').forEach((row) => {
      const isSel = row.dataset.srv === id;
      row.classList.toggle('selected', isSel);
      const nameEl = row.querySelector('.grow div:first-child');
      if (nameEl) {
        const base = nameEl.dataset.baseName || nameEl.textContent;
        nameEl.dataset.baseName = base;
        nameEl.innerHTML = esc(base) + (isSel
          ? ` <span class="muted">· ${esc(t('servers.viewing'))}</span>` : '');
      }
    });
    this._renderConsole();
  }

  _dialog() {
    const modal = openModal({
      title: t('servers.createTitle'),
      icon: 'servers',
      tone: 'accent',
      body: `
        <div class="field">
          <label for="ns-name">${t('servers.name')}</label>
          <input type="text" id="ns-name" placeholder="My Server" />
        </div>
        <div class="grid-2 stagger">
          <div class="field">
            <label for="ns-soft">${t('servers.software')}</label>
            <select id="ns-soft">${SOFTWARE.map((s) => `<option value="${s}">${s}</option>`).join('')}</select>
          </div>
          <div class="field">
            <label for="ns-version">${t('servers.version')}</label>
            <input type="text" id="ns-version" value="1.21" />
          </div>
        </div>
        <div class="grid-2 stagger">
          <div class="field">
            <label for="ns-ram">${t('servers.ram')}</label>
            <input type="text" id="ns-ram" value="2048" />
          </div>
          <div class="field">
            <label for="ns-port">Port</label>
            <input type="text" id="ns-port" value="25565" />
          </div>
        </div>`,
      actions: [
        { label: t('accounts.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('instances.create'),
          variant: 'success',
          icon: 'check',
          onClick: async ({ close }) => {
            const name = modal.el.querySelector('#ns-name').value.trim();
            const software = modal.el.querySelector('#ns-soft').value;
            const version = modal.el.querySelector('#ns-version').value.trim();
            const ram = parseInt(modal.el.querySelector('#ns-ram').value, 10) || 2048;
            const port = parseInt(modal.el.querySelector('#ns-port').value, 10) || 25565;
            if (!name) { toastError(t('servers.error.name')); return; }
            const res = await call('servers_create', name, software, version, { ramMb: ram, port });
            if (!res.ok) { handleError(res.error); return; }
            toastSuccess(t('servers.toast.created'));
            close();
            this.mount();
          },
        },
      ],
    });
    requestAnimationFrame(() => modal.el.querySelector('#ns-name')?.focus());
  }

  async _start(id) {
    const res = await call('servers_start', id);
    if (!res.ok) { handleError(res.error); return; }
    this._selectedId = id;
    toastSuccess(t('servers.toast.starting'));
    this.mount();
  }

  async _send() {
    const input = this.el.querySelector('#srv-cmd');
    if (!input) return;
    const cmd = input.value.trim();
    if (!cmd) return;
    if (this._selectedId) await call('servers_command', this._selectedId, cmd);
    input.value = '';
  }

  /** servers.changed -> nạp lại list (debounce 150ms để gộp burst). */
  _reloadListSoon() {
    if (this._reloadTimer) clearTimeout(this._reloadTimer);
    this._reloadTimer = setTimeout(() => {
      this._reloadTimer = null;
      this._reloadList();
    }, 150);
  }

  async _reloadList() {
    const res = await call('servers_list');
    if (!res?.ok || !this.el?.isConnected) return;
    const servers = res.data.servers || [];
    const card = this.el.querySelector('.card');
    if (!card) return;
    card.innerHTML = servers.length === 0
      ? `<div class="empty">
           <div class="big">${icon('servers', 26)}</div>
           <div class="empty-title">${t('servers.none')}</div>
         </div>`
      : servers.map((s) => this._serverRow(s)).join('');
    this._bindRowActions(card);
  }

  _bindRowActions(root) {
    // Chọn server: click/Enter/Space trên hàng — nút bên trong không trigger chọn
    root.querySelectorAll('[data-srv]').forEach((row) => {
      row.addEventListener('click', (e) => {
        if (e.target.closest('button')) return;   // start/stop/delete giữ hành vi riêng
        this._selectServer(row.dataset.srv);
      });
      row.addEventListener('keydown', (e) => {
        if (e.key !== 'Enter' && e.key !== ' ') return;
        e.preventDefault();
        this._selectServer(row.dataset.srv);
      });
    });
    root.querySelectorAll('[data-start]').forEach((b) =>
      b.addEventListener('click', () => this._start(b.dataset.start)));
    root.querySelectorAll('[data-stop]').forEach((b) =>
      b.addEventListener('click', async () => {
        const r = await call('servers_stop', b.dataset.stop);
        if (!r.ok) { handleError(r.error); return; }
        this.mount();
      }));
    root.querySelectorAll('[data-del]').forEach((b) =>
      b.addEventListener('click', async () => {
        const ok = await confirmDialog({
          title: t('common.delete'),
          message: t('servers.confirmDelete'),
          confirmText: t('common.delete'),
        });
        if (!ok) return;
        const r = await call('servers_delete', b.dataset.del, true);
        if (!r.ok) { handleError(r.error); return; }
        toastSuccess(t('servers.toast.deleted'));
        this._consoles.delete(b.dataset.del);
        this._metrics.delete(b.dataset.del);
        this.mount();
      }));
  }

  _syncJump() {
    const following = this._view?.following ?? true;
    this._jumpEl?.classList.toggle('show', !following);
    // Khi đang pause: ẩn jump, hiện resume + số dòng mới (nếu có)
    if (this._resumeEl) {
      this._resumeEl.style.display = following ? 'none' : 'inline-flex';
      if (this._newCountEl) {
        this._newCountEl.textContent = this._pausedNew > 0
          ? `+${this._pausedNew}` : '';
      }
    }
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    if (this._reloadTimer) clearTimeout(this._reloadTimer);
    this._reloadTimer = null;
    this._view?.destroy();
    this._view = null;
  }
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

/**
 * Mức dòng console server (Minecraft/Proxy — mục 16, 151).
 * Trả class màu: err | warn | info | ok | '' — giống Logs tab.
 */
function classifyServerLine(text) {
  const s = String(text ?? '');
  // Lỗi nghiêm trọng trước — vài dòng ERROR cũng chứa WARN/INFO
  if (/\b(ERROR|FATAL|CRITICAL)\b|Exception|at [\w$.]+\(/.test(s)) return 'err';
  if (/\bWARN(ING)?\b/.test(s)) return 'warn';
  // Trạng thái sẵn sàng trước INFO: "Done (2.31s)!" luôn kèm INFO nhưng đáng
  // tô xanh hơn — là tín hiệu server đã lên.
  if (/\bDone \(|listening on/.test(s)) return 'ok';
  // Chat: "<Steve> message" (vanilla) — tô trắng nổi bật hơn log thường
  if (/<[A-Za-z0-9_]{3,16}>/.test(s)) return 'chat';
  if (/\bINFO\b/.test(s)) return 'info';
  return '';
}
