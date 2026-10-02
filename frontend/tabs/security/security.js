/**
 * Security tab — mod security & health center.
 * - Quét mods dir: bytecode scan + verdict + auto-quarantine DANGEROUS
 * - Health: thiếu deps / wrong loader / outdated + Auto-fix (Fabric API...)
 * - Upload jar phân tích sâu
 * - Quản lý quarantine (restore / delete vĩnh viễn)
 */
import { call } from '../../app/bridge.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { confirmDialog } from '../../app/modal.js';
import { onBackendEvent } from '../../app/events.js';

export class SecurityView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._taskId = null;
    this._expect = null;   // {type, owner} của task đang chờ nhận
  }

  /**
   * Nhận task theo id, hoặc theo (type, owner) khi event phát ra nhanh hơn
   * lệnh trả về (taskId chưa kịp gán).
   */
  _accepts(task) {
    if (!task) return false;
    if (this._taskId && task.id === this._taskId) return true;
    const e = this._expect;
    return Boolean(e && task.type === e.type && task.owner === e.owner);
  }

  _beginTask(type, instanceId, taskId) {
    this._expect = { type, owner: `instance:${instanceId}` };
    this._taskId = taskId || null;
  }

  async mount() {
    const state = await call('app_get_state');
    const instances = state.ok ? (state.data.instancesSummary || []) : [];
    const selected = instances.find(i => i.id === state.data.selectedInstance) || instances[0];

    this.el.innerHTML = `
      <div class="view-title">${t('security.title')}</div>
      <div class="view-sub">${t('security.subtitle')}</div>

      <!-- Scan controls -->
      <div class="card">
        <div class="grid-2 stagger">
          <div class="field"><label>${t('mods.instance')}</label>
            <select id="sec-inst">
              ${instances.map(i => `<option value="${i.id}" ${i === selected ? 'selected' : ''}>
                ${i.name} (${i.loader || 'vanilla'} ${i.version || ''})</option>`).join('')}
            </select></div>
          <div class="field"><label>${t('security.upload')}</label>
            <input type="file" id="sec-file" accept=".jar" style="padding:6px" /></div>
        </div>
        <div style="display:flex;gap:8px;flex-wrap:wrap">
          <button class="btn primary" id="sec-scan">${icon('shieldCheck', 15, 'btn-icon')}${t('security.scan')}</button>
          <button class="btn" id="sec-health">${t('security.health')}</button>
          <button class="btn" id="sec-autofix">${t('security.autofix')}</button>
          <button class="btn ghost" id="sec-quarantine">${t('security.quarantine')}</button>
        </div>
        <div class="progress" id="sec-progress" style="display:none;margin-top:12px"><div style="width:0%"></div></div>
        <div class="muted" id="sec-status" style="margin-top:6px"></div>
      </div>

      <!-- Results -->
      <div class="card section">
        <h3>${icon('shieldCheck', 14)} ${t('security.results')}</h3>
        <div id="sec-results"><div class="empty">${t('security.hint')}</div></div>
      </div>

      <!-- Health results -->
      <div class="card section hidden" id="sec-health-card">
        <h3>${icon('diagnostics', 14)} ${t('security.healthResults')}</h3>
        <div id="sec-health-body"></div>
      </div>

      <!-- Quarantine -->
      <div class="card section hidden" id="sec-q-card">
        <h3>${icon('security', 14)} ${t('security.quarantineTitle')}</h3>
        <div id="sec-q-body"></div>
      </div>
    `;

    this.el.querySelector('#sec-scan').addEventListener('click', () => this._scan());
    this.el.querySelector('#sec-health').addEventListener('click', () => this._health());
    this.el.querySelector('#sec-autofix').addEventListener('click', () => this._autofix());
    this.el.querySelector('#sec-quarantine').addEventListener('click', () => this._showQuarantine());
    this.el.querySelector('#sec-file').addEventListener('change', (e) => this._upload(e));

    // Tiến trình quét/tự sửa đến từ event — không còn poll tasks_list (mục 15.2)
    this._unsubs.push(onBackendEvent('task.updated', (e) => this._onTaskEvent(e.payload)));
  }

  // ---------- Scan instance ----------
  async _scan() {
    const instId = this.el.querySelector('#sec-inst').value;
    if (!instId) { toastError(t('mods.selectInstance')); return; }
    this._beginTask('MOD_SCAN', instId, null);
    const res = await call('modscan_instance', instId);
    if (!res.ok) { this._expect = null; handleError(res.error); return; }
    this._taskId = res.data.taskId;
    this.el.querySelector('#sec-progress').style.display = 'block';
    this.el.querySelector('#sec-status').textContent = t('security.scanning');
  }

  _onTaskEvent(task) {
    if (!this._accepts(task)) return;
    this._taskId = task.id;
    const bar = this.el.querySelector('#sec-progress');
    const fill = bar?.querySelector('div');
    if (fill) fill.style.width = `${task.progress}%`;
    this.el.querySelector('#sec-status').textContent = task.message || task.state;

    if (task.state === 'completed') {
      this._taskId = null;
      this._expect = null;
      this._renderResults(task.result);
      this.el.querySelector('#sec-progress').style.display = 'none';
    } else if (task.state === 'failed') {
      this._taskId = null;
      this._expect = null;
      handleError(task.error);
    }
  }

  _renderResults(result) {
    const box = this.el.querySelector('#sec-results');
    if (!result || !result.results) { box.innerHTML = `<div class="empty">—</div>`; return; }
    const { results, dangerous, suspicious, safe } = result;

    const summary = `
      <div class="wrap inline" style="margin-bottom:12px">
        <span class="badge online">${t('security.safe')}: ${safe}</span>
        <span class="badge warn">${t('security.suspicious')}: ${suspicious}</span>
        <span class="badge offline">${t('security.dangerous')}: ${dangerous}</span>
      </div>`;

    // Chặn render hàng nghìn jar cùng lúc (mục 16) — phần vượt hiển thị số lượng
    const LIMIT = 300;
    const shown = results.slice(0, LIMIT);
    const note = results.length > LIMIT
      ? `<div class="muted" style="margin:0 0 10px">${t('security.truncated', { shown: shown.length, total: results.length })}</div>`
      : '';

    const rows = shown.map(r => {
      const badge = r.verdict === 'DANGEROUS' ? 'offline'
        : r.verdict === 'SUSPICIOUS' ? '' : 'online';
      const qBadge = r.quarantined ? ` <span class="badge accent">${t('security.quarantined')}</span>` : '';
      const findings = (r.findings || []).map(f => `
        <div class="finding sev-${escapeHtml(f.severity)}">
          <span class="finding-sev">${escapeHtml(f.severity)}</span>
          <span class="grow">${escapeHtml(f.title)}
            <span class="finding-rule">${escapeHtml(f.ruleId)}</span>
          </span>
        </div>`).join('');
      return `
        <div class="scan-row" style="flex-direction:column;align-items:stretch">
          <div style="display:flex;gap:8px;align-items:center;width:100%">
            <span class="badge ${badge}">${r.verdict} (${r.score})</span>
            <span class="grow"><b>${escapeHtml(r.file)}</b>${qBadge}</span>
            <span class="muted">${r.classesScanned} classes · Java ${r.javaVersion || '?'}</span>
          </div>
          ${findings}
        </div>`;
    }).join('');

    box.innerHTML = summary + note + (rows || `<div class="empty">${t('mods.noInstalled')}</div>`);
  }

  // ---------- Health ----------
  async _health() {
    const instId = this.el.querySelector('#sec-inst').value;
    if (!instId) return;
    const card = this.el.querySelector('#sec-health-card');
    const body = this.el.querySelector('#sec-health-body');
    card.classList.remove('hidden');
    body.innerHTML = `<div class="empty"><span class="spinner"></span>${t('mods.searching')}</div>`;

    const res = await call('mods_health', instId, true);
    if (!res.ok) { handleError(res.error); body.innerHTML = ''; return; }
    const { issues, outdated, mods } = res.data;

    let html = `<div class="muted" style="margin-bottom:8px">${mods.length} ${t('security.modsScanned')}</div>`;
    if (issues.length === 0 && outdated.length === 0) {
      html += `<div class="empty">✅ ${t('security.allHealthy')}</div>`;
    } else {
      for (const i of issues) {
        const badge = i.kind === 'missing_dependency' ? 'accent' : 'offline';
        html += `<div class="scan-row"><span class="badge ${badge}">${i.kind}</span>
          <span class="grow">${escapeHtml(i.mod)}<br><span class="muted">${escapeHtml(i.detail)}</span></span>
          ${i.fixable ? `<span class="badge online">auto-fixable</span>` : ''}</div>`;
      }
      for (const o of outdated) {
        html += `<div class="scan-row"><span class="badge info">outdated</span>
          <span class="grow">${escapeHtml(o.mod)}<br><span class="muted">${escapeHtml(o.detail)}</span></span></div>`;
      }
    }
    body.innerHTML = html;
  }

  // ---------- Auto-fix ----------
  async _autofix() {
    const instId = this.el.querySelector('#sec-inst').value;
    if (!instId) return;
    this._beginTask('MOD_AUTOFIX', instId, null);
    const res = await call('mods_autofix', instId);
    if (!res.ok) { this._expect = null; handleError(res.error); return; }
    this._taskId = res.data.taskId;
    this.el.querySelector('#sec-progress').style.display = 'block';
    this.el.querySelector('#sec-status').textContent = t('security.autofixing');
  }

  // ---------- Upload ----------
  async _upload(e) {
    const file = e.target.files?.[0];
    if (!file) return;
    const status = this.el.querySelector('#sec-status');
    status.textContent = t('security.uploading');

    const content_b64 = await this._fileToBase64(file);
    const res = await call('modscan_upload', file.name, content_b64);
    if (!res.ok) { handleError(res.error); return; }

    const r = res.data.report;
    const badge = r.verdict === 'DANGEROUS' ? 'offline'
      : r.verdict === 'SUSPICIOUS' ? '' : 'online';
    const findings = (r.findings || []).map(f => `
      <div class="finding sev-${escapeHtml(f.severity)}">
        <span class="finding-sev">${escapeHtml(f.severity)}</span>
        <span class="grow">${escapeHtml(f.title)}
          <span class="finding-rule">${escapeHtml(f.ruleId)}</span>
        </span>
      </div>`).join('');

    this.el.querySelector('#sec-results').innerHTML = `
      <div class="scan-row" style="flex-direction:column;align-items:stretch">
        <div style="display:flex;gap:8px;align-items:center;width:100%">
          <span class="badge ${badge}">${r.verdict} (${r.score})</span>
          <span class="grow"><b>${escapeHtml(r.file)}</b></span>
          <span class="muted">${r.classesScanned} classes · Java ${r.javaVersion || '?'}</span>
        </div>
        ${findings || `<div class="muted">✅ ${t('security.noFindings')}</div>`}
      </div>`;
    status.textContent = t('security.uploadDone');
    e.target.value = '';
  }

  _fileToBase64(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = String(reader.result);
        resolve(result.includes(',') ? result.split(',')[1] : result);
      };
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }

  // ---------- Quarantine ----------
  async _showQuarantine() {
    const card = this.el.querySelector('#sec-q-card');
    const body = this.el.querySelector('#sec-q-body');
    card.classList.remove('hidden');
    const res = await call('quarantine_list');
    if (!res.ok) { handleError(res.error); return; }
    const items = res.data.items || [];
    if (items.length === 0) {
      body.innerHTML = `<div class="empty">${t('security.quarantineEmpty')}</div>`;
      return;
    }
    body.innerHTML = items.map(q => `
      <div class="row">
        <span class="badge offline">${q.verdict} (${q.score})</span>
        <span class="grow"><b>${escapeHtml(q.originalName)}</b><br>
          <span class="muted">${escapeHtml(q.originalPath)}</span></span>
        <button class="btn ghost sm" data-restore="${q.quarantineFile}">${icon('refresh', 14, 'btn-icon')}${t('security.restore')}</button>
        <button class="btn danger-ghost sm" data-delete="${q.quarantineFile}" aria-label="${t('common.delete')}">${icon('trash', 15)}</button>
      </div>`).join('');

    body.querySelectorAll('[data-restore]').forEach(b =>
      b.addEventListener('click', async () => {
        const r = await call('quarantine_restore', b.dataset.restore);
        r.ok ? toastSuccess(t('security.restored')) : handleError(r.error);
        this._showQuarantine();
      }));

    body.querySelectorAll('[data-delete]').forEach(b =>
      b.addEventListener('click', async () => {
        const ok = await confirmDialog({
          title: t('common.delete'),
          message: t('security.confirmDelete'),
          confirmText: t('common.delete'),
        });
        if (!ok) return;
        const r = await call('quarantine_delete', b.dataset.delete);
        r.ok ? toastSuccess(t('security.restored')) : handleError(r.error);
        this._showQuarantine();
      }));
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._taskId = null;
    this._expect = null;
  }
}

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, c => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
