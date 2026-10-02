/**
 * Profiles tab — Player Profiles: 1 preset = account + instance + JVM +
 * game settings + launch options. Apply 1 click, plan trước khi áp (mục 40).
 */
import { call } from '../../app/bridge.js';
import { toastSuccess, toastError, toastInfo } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { openModal, confirmDialog } from '../../app/modal.js';
import { onBackendEvents } from '../../app/events.js';

const JVM_PRESETS = ['auto', 'default', 'balanced', 'low_memory', 'performance'];

/** Cặp [specKey, inputId, i18nKey] cho khối Game settings (whitelist backend). */
const GAME_FIELDS = [
  ['gamma', '#pf-gamma', 'profiles.gamma'],
  ['fov', '#pf-fov', 'profiles.fov'],
  ['guiScale', '#pf-guiscale', 'profiles.guiScale'],
  ['maxFps', '#pf-maxfps', 'profiles.maxFps'],
  ['renderDistance', '#pf-rd', 'profiles.renderDistance'],
  ['simulationDistance', '#pf-sd', 'profiles.simulationDistance'],
  ['sensitivity', '#pf-sens', 'profiles.sensitivity'],
  ['soundCategory_master', '#pf-vol', 'profiles.masterVolume'],
];

export class ProfilesView {
  constructor(el) {
    this.el = el;
    this._profiles = [];
    this._state = null;
    this._query = '';
    this._unsubs = [];
    this._reloadTimer = null;
  }

  async mount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];

    const [res, state] = await Promise.all([call('profiles_list'), call('app_get_state')]);
    this._profiles = res?.ok ? (res.data.profiles || []) : [];
    this._state = state?.ok ? state.data : null;

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('profiles.title')}</div>
          <div class="view-sub">${t('profiles.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="pf-capture">${icon('copy', 15, 'btn-icon')}${t('profiles.capture')}</button>
          <button class="btn" id="pf-import">${icon('upload', 15, 'btn-icon')}${t('profiles.import')}</button>
          <button class="btn primary" id="pf-new">${icon('plus', 15, 'btn-icon')}${t('profiles.new')}</button>
          <input type="file" id="pf-file" accept=".json" class="hidden" />
        </div>
      </div>

      <div class="toolbar">
        <input type="text" id="pf-search" class="grow"
               placeholder="${esc(t('profiles.searchPlaceholder'))}" value="${esc(this._query)}" />
        <button class="btn" id="pf-revert" data-tip="${esc(t('profiles.revertHint'))}">
          ${icon('refresh', 15, 'btn-icon')}${t('profiles.revert')}
        </button>
      </div>

      <div class="row-between" style="margin-bottom:10px">
        <span class="vlist-count" id="pf-count"></span>
      </div>

      <div id="prof-list" class="stagger"></div>`;

    this.el.querySelector('#pf-new')?.addEventListener('click', () => this._dialog(null));
    this.el.querySelector('#pf-capture')?.addEventListener('click', () => this._captureDialog());
    this.el.querySelector('#pf-import')?.addEventListener('click', () => {
      this.el.querySelector('#pf-file')?.click();
    });
    this.el.querySelector('#pf-file')?.addEventListener('change', (e) => this._importFile(e));
    this.el.querySelector('#pf-revert')?.addEventListener('click', () => this._revert());
    this.el.querySelector('#pf-search')?.addEventListener('input', (e) => {
      this._query = e.target.value.trim().toLowerCase();
      this._renderList();
    });

    this._renderList();

    this._unsubs.push(
      onBackendEvents(['profiles.changed', 'profile.applied', 'profile.reverted'], () => {
        this._reloadSoon();
      }),
    );
  }

  _reloadSoon() {
    if (this._reloadTimer) clearTimeout(this._reloadTimer);
    this._reloadTimer = setTimeout(async () => {
      this._reloadTimer = null;
      const res = await call('profiles_list');
      if (!res?.ok || !this.el?.isConnected) return;
      this._profiles = res.data.profiles || [];
      this._renderList();
    }, 150);
  }

  _filtered() {
    if (!this._query) return this._profiles;
    return this._profiles.filter((p) => p.name.toLowerCase().includes(this._query));
  }

  _renderList() {
    const box = this.el.querySelector('#prof-list');
    if (!box) return;
    const items = this._filtered();
    const count = this.el.querySelector('#pf-count');
    if (count) count.textContent = t('common.showing', { shown: items.length, total: this._profiles.length });
    if (!items.length) {
      box.innerHTML = `<div class="empty">
        <div class="big">${icon('star', 26)}</div>
        <div class="empty-title">${t('profiles.none')}</div>
        <div>${t('profiles.noneSub')}</div>
      </div>`;
      box.onclick = null;
      box.onkeydown = null;
      return;
    }
    box.innerHTML = items.map((p) => this._card(p)).join('');
    box.onclick = (e) => this._onClick(e);
    box.onkeydown = (e) => {
      if (e.key !== 'Enter' && e.key !== ' ') return;
      const card = e.target.closest('[data-id]');
      if (card) { e.preventDefault(); this._planApply(card.dataset.id); }
    };
  }

  _card(p) {
    const spec = p.spec || {};
    const summary = [];
    if (spec.account?.id) {
      const acc = (this._state?.accountsSummary || []).find((a) => a.id === spec.account.id);
      summary.push(`${icon('user', 12)} ${esc(acc?.displayName || spec.account.id)}`);
    }
    if (spec.instance?.id) {
      const inst = (this._state?.instancesSummary || []).find((i) => i.id === spec.instance.id);
      summary.push(`${icon('instances', 12)} ${esc(inst?.name || spec.instance.id)}`);
    }
    if (spec.game) summary.push(`${icon('settings', 12)} ${Object.keys(spec.game).length}`);
    if (spec.launch?.server) summary.push(`${icon('servers', 12)} ${esc(spec.launch.server)}`);
    return `
      <div class="inst-card ${p.active ? 'selected' : ''}" data-id="${esc(p.id)}" role="button" tabindex="0">
        <div class="inst-top">
          <span class="inst-mark">${icon('star', 18)}</span>
          <span class="grow">
            <div class="inst-name truncate">${esc(p.name)}</div>
            <div class="muted truncate">${summary.join(' ') || t('profiles.emptySpec')}</div>
          </span>
        </div>
        <div class="inst-foot">
          ${p.active ? `<span class="badge online">${icon('check', 12)}</span>` : ''}
          <span class="grow"></span>
          <button class="btn sm" data-edit="${esc(p.id)}" data-tip="${esc(t('profiles.edit'))}">${icon('edit', 14)}</button>
          <button class="btn sm" data-export="${esc(p.id)}" data-tip="${esc(t('profiles.export'))}">${icon('externalLink', 14)}</button>
          <button class="btn sm" data-dup="${esc(p.id)}" data-tip="${esc(t('profiles.duplicate'))}">${icon('copy', 14)}</button>
          <button class="btn danger-ghost sm" data-del="${esc(p.id)}" data-tip="${esc(t('common.delete'))}">${icon('trash', 14)}</button>
          <button class="btn primary sm" data-apply="${esc(p.id)}">${icon('play', 14, 'btn-icon')}${t('profiles.apply')}</button>
        </div>
      </div>`;
  }

  async _onClick(e) {
    const del = e.target.closest('[data-del]');
    if (del) { e.stopPropagation(); await this._remove(del.dataset.del); return; }
    const edit = e.target.closest('[data-edit]');
    if (edit) { e.stopPropagation(); this._dialog(edit.dataset.edit); return; }
    const dup = e.target.closest('[data-dup]');
    if (dup) { e.stopPropagation(); await this._duplicate(dup.dataset.dup); return; }
    const exp = e.target.closest('[data-export]');
    if (exp) { e.stopPropagation(); this._export(exp.dataset.export); return; }
    const apply = e.target.closest('[data-apply]');
    if (apply) { e.stopPropagation(); await this._planApply(apply.dataset.apply); return; }
    const card = e.target.closest('[data-id]');
    if (card) await this._planApply(card.dataset.id);
  }

  _instanceName(id) {
    const inst = (this._state?.instancesSummary || []).find((i) => i.id === id);
    return inst?.name || id;
  }

  /** Plan -> modal "What will change?" -> confirm mới apply (mục 39, 70). */
  async _planApply(id) {
    const plan = await call('profiles_plan', id);
    if (!plan.ok) { handleError(plan.error); return; }
    const c = plan.data.changes || {};
    const rows = [];
    if (c.account) rows.push([t('profiles.account'), c.account.before || '—', c.account.after || '—']);
    if (c.instance) rows.push([t('profiles.instance'),
      c.instance.before ? this._instanceName(c.instance.before) : '—', c.instance.afterName || '—']);
    for (const ch of c.jvm || []) {
      rows.push([esc(ch.field), this._short(ch.before), this._short(ch.after)]);
    }
    for (const ch of c.game || []) rows.push([esc(ch.field), ch.before ?? '—', ch.after ?? '—']);
    for (const ch of c.launch || []) rows.push([esc(ch.field), ch.before, this._short(ch.after)]);

    const modal = openModal({
      title: t('profiles.planTitle', { name: plan.data.profileName || id }),
      icon: 'play',
      tone: 'accent',
      body: rows.length
        ? `<div class="plan-table">${rows.map(([f, b, a]) => `
            <div class="plan-row"><span class="plan-field">${f}</span>
            <span class="plan-before">${b}</span>
            <span class="plan-arrow">${icon('arrowRight', 12)}</span>
            <span class="plan-after">${a}</span></div>`).join('')}</div>`
        : `<div class="empty"><div>${t('profiles.noChanges')}</div></div>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('profiles.apply'), variant: 'success', icon: 'check',
          onClick: async ({ close }) => {
            const r = await call('profiles_apply', id);
            if (!r.ok) { handleError(r.error); return; }
            close();
            toastSuccess(t('profiles.applied', { name: plan.data.profileName || id }));
            this.mount();
          },
        },
      ],
    });
    requestAnimationFrame(() => modal.el.querySelector('[data-primary], .btn.success')?.focus());
  }

  _short(v) {
    if (v === null || v === undefined) return '—';
    if (typeof v === 'object') {
      if (typeof v.minMb === 'number' && typeof v.maxMb === 'number') return `${v.minMb}-${v.maxMb} MB`;
      return JSON.stringify(v);
    }
    return String(v);
  }

  async _revert() {
    const r = await call('profiles_revert');
    if (!r.ok) { handleError(r.error); return; }
    toastInfo(t('profiles.reverted'));
    this.mount();
  }

  async _duplicate(id) {
    const r = await call('profiles_duplicate', id);
    if (!r.ok) { handleError(r.error); return; }
    toastSuccess(t('profiles.duplicated', { name: r.data.profile.name }));
    this.mount();
  }

  async _remove(id) {
    const p = this._profiles.find((x) => x.id === id);
    const ok = await confirmDialog({
      title: t('profiles.deleteTitle'),
      message: t('profiles.confirmDelete', { name: p?.name || id }),
      confirmText: t('common.delete'),
    });
    if (!ok) return;
    const r = await call('profiles_delete', id, true);
    if (!r.ok) { handleError(r.error); return; }
    toastSuccess(t('profiles.deleted'));
    this.mount();
  }

  _export(id) {
    const p = this._profiles.find((x) => x.id === id);
    const payload = {
      format: 'antares-profile', version: 1,
      name: p?.name || id, spec: p?.spec || {},
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${(p?.name || 'profile').replace(/[\\/:*?"<>|]/g, '_')}.antares-profile.json`;
    a.click();
    URL.revokeObjectURL(url);
    toastSuccess(t('profiles.exported'));
  }

  async _importFile(e) {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    try {
      const data = JSON.parse(await file.text());
      const r = await call('profiles_import', data);
      if (!r.ok) { handleError(r.error); return; }
      toastSuccess(t('profiles.imported', { name: r.data.profile.name }));
      this.mount();
    } catch (err) {
      toastError(t('profiles.importFailed'));
    }
  }

  /** Modal tạo/capture — chọn instance đang có để chụp JVM + game + account. */
  _captureDialog() {
    const instances = this._state?.instancesSummary || [];
    if (!instances.length) { toastError(t('profiles.noInstances')); return; }
    const modal = openModal({
      title: t('profiles.captureTitle'),
      icon: 'copy',
      tone: 'accent',
      body: `
        <div class="field">
          <label for="pf-c-inst">${t('profiles.instance')}</label>
          <select id="pf-c-inst">${instances.map((i) =>
            `<option value="${esc(i.id)}">${esc(i.name)} (${esc(i.version || '?')})</option>`).join('')}</select>
        </div>
        <div class="field">
          <label for="pf-c-name">${t('profiles.name')}</label>
          <input type="text" id="pf-c-name" placeholder="${esc(t('profiles.namePlaceholder'))}" />
        </div>
        <p class="muted">${t('profiles.captureHint')}</p>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('profiles.capture'), variant: 'success', icon: 'check',
          onClick: async ({ close }) => {
            const instId = modal.el.querySelector('#pf-c-inst').value;
            const name = modal.el.querySelector('#pf-c-name').value.trim();
            const r = await call('profiles_capture', instId, name);
            if (!r.ok) { handleError(r.error); return; }
            close();
            toastSuccess(t('profiles.captured', { name: r.data.profile.name }));
            this.mount();
          },
        },
      ],
    });
    requestAnimationFrame(() => modal.el.querySelector('#pf-c-name')?.focus());
  }

  /** Modal tạo/sửa — account/instance + JVM + game + launch. */
  _dialog(profileId) {
    const p = this._profiles.find((x) => x.id === profileId);
    const spec = p?.spec || {};
    const accounts = this._state?.accountsSummary || [];
    const instances = this._state?.instancesSummary || [];
    const jvm = spec.jvm || {};
    const game = spec.game || {};
    const launch = spec.launch || {};
    const gameInput = (key, sel, labelKey, ph = '') => `
      <div class="field">
        <label for="${sel.slice(1)}">${t(labelKey)}</label>
        <input type="text" id="${sel.slice(1)}" placeholder="${esc(ph)}" value="${esc(game[key] ?? '')}" />
      </div>`;

    const modal = openModal({
      title: profileId ? t('profiles.editTitle', { name: p.name }) : t('profiles.createTitle'),
      icon: 'star',
      tone: 'accent',
      wide: true,
      body: `
        <div class="field">
          <label for="pf-name">${t('profiles.name')}</label>
          <input type="text" id="pf-name" value="${esc(p?.name || '')}"
                 placeholder="${esc(t('profiles.namePlaceholder'))}" />
        </div>
        <div class="grid-2">
          <div class="field">
            <label for="pf-account">${t('profiles.account')}</label>
            <select id="pf-account">
              <option value="">—</option>
              ${accounts.map((a) => `<option value="${esc(a.id)}" ${spec.account?.id === a.id ? 'selected' : ''}>${esc(a.displayName)}</option>`).join('')}
            </select>
          </div>
          <div class="field">
            <label for="pf-instance">${t('profiles.instance')}</label>
            <select id="pf-instance">
              <option value="">—</option>
              ${instances.map((i) => `<option value="${esc(i.id)}" ${spec.instance?.id === i.id ? 'selected' : ''}>${esc(i.name)}</option>`).join('')}
            </select>
          </div>
        </div>

        <div class="field-group"><div class="field-group-title">${t('profiles.jvm')}</div>
          <div class="grid-3">
            <div class="field">
              <label for="pf-rammin">${t('profiles.ramMin')}</label>
              <input type="text" id="pf-rammin" value="${esc(jvm.memory?.minMb ?? '')}" placeholder="512" />
            </div>
            <div class="field">
              <label for="pf-rammax">${t('profiles.ramMax')}</label>
              <input type="text" id="pf-rammax" value="${esc(jvm.memory?.maxMb ?? '')}" placeholder="2048" />
            </div>
            <div class="field">
              <label for="pf-preset">${t('profiles.jvmPreset')}</label>
              <select id="pf-preset">${JVM_PRESETS.map((x) =>
                `<option value="${x}" ${jvm.jvmPreset === x ? 'selected' : ''}>${x}</option>`).join('')}</select>
            </div>
          </div>
          <div class="field">
            <label for="pf-jvmargs">${t('profiles.jvmArgs')}</label>
            <input type="text" id="pf-jvmargs" value="${esc((jvm.jvmArgs || []).join(' '))}"
                   placeholder="-XX:+UseG1GC -XX:MaxGCPauseMillis=50" />
          </div>
        </div>

        <div class="field-group"><div class="field-group-title">${t('profiles.game')}</div>
          <div class="grid-3">
            ${gameInput('gamma', '#pf-gamma', 'profiles.gamma', '1.0')}
            ${gameInput('fov', '#pf-fov', 'profiles.fov', '0.0')}
            ${gameInput('guiScale', '#pf-guiscale', 'profiles.guiScale', '0')}
            ${gameInput('maxFps', '#pf-maxfps', 'profiles.maxFps', '260')}
            ${gameInput('renderDistance', '#pf-rd', 'profiles.renderDistance', '12')}
            ${gameInput('simulationDistance', '#pf-sd', 'profiles.simulationDistance', '10')}
            ${gameInput('sensitivity', '#pf-sens', 'profiles.sensitivity', '1.0')}
            ${gameInput('soundCategory_master', '#pf-vol', 'profiles.masterVolume', '1.0')}
          </div>
        </div>

        <div class="field-group"><div class="field-group-title">${t('profiles.launch')}</div>
          <div class="grid-2">
            <div class="field">
              <label for="pf-server">${t('profiles.server')}</label>
              <input type="text" id="pf-server" value="${esc(launch.server || '')}" placeholder="mc.example.net" />
            </div>
            <div class="field">
              <label for="pf-port">${t('profiles.port')}</label>
              <input type="text" id="pf-port" value="${esc(launch.port || '')}" placeholder="25565" />
            </div>
          </div>
          <div class="grid-2">
            <div class="field">
              <label for="pf-quickworld">${t('profiles.quickPlayWorld')}</label>
              <input type="text" id="pf-quickworld" value="${esc(launch.quickPlaySingleplayer || '')}" />
            </div>
            <div class="field">
              <label for="pf-quickserver">${t('profiles.quickPlayServer')}</label>
              <input type="text" id="pf-quickserver" value="${esc(launch.quickPlayMultiplayer || '')}" />
            </div>
          </div>
          <label class="check-row">
            <input type="checkbox" id="pf-cres" ${launch.customResolution ? 'checked' : ''} />
            <span>${t('profiles.customResolution')}</span>
          </label>
          <div class="grid-2">
            <div class="field">
              <label for="pf-rw">${t('profiles.resWidth')}</label>
              <input type="text" id="pf-rw" value="${esc(launch.resolutionWidth || '854')}" />
            </div>
            <div class="field">
              <label for="pf-rh">${t('profiles.resHeight')}</label>
              <input type="text" id="pf-rh" value="${esc(launch.resolutionHeight || '480')}" />
            </div>
          </div>
        </div>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: profileId ? t('common.save') : t('profiles.create'),
          variant: 'success', icon: 'check',
          onClick: async ({ close }) => {
            const name = modal.el.querySelector('#pf-name').value.trim();
            if (!name) { toastError(t('profiles.error.name')); return; }
            const spec = this._collectSpec(modal);
            const r = profileId
              ? await call('profiles_update', profileId, { name, spec })
              : await call('profiles_create', name, spec);
            if (!r.ok) { handleError(r.error); return; }
            close();
            toastSuccess(t(profileId ? 'profiles.edited' : 'profiles.created', { name }));
            this.mount();
          },
        },
      ],
    });
    requestAnimationFrame(() => modal.el.querySelector('#pf-name')?.focus());
  }

  /** Đọc input modal -> spec dict (backend sanitize/coerce, UI không tự chế). */
  _collectSpec(modal) {
    const v = (sel) => modal.el.querySelector(sel)?.value?.trim() || '';
    const spec = {};
    const accId = v('#pf-account');
    if (accId) spec.account = { id: accId };
    const instId = v('#pf-instance');
    if (instId) spec.instance = { id: instId };

    const jvm = {};
    const memMin = parseInt(v('#pf-rammin'), 10);
    const memMax = parseInt(v('#pf-rammax'), 10);
    if (memMin && memMax) jvm.memory = { minMb: memMin, maxMb: memMax };
    const preset = v('#pf-preset');
    if (preset) jvm.jvmPreset = preset;
    const args = v('#pf-jvmargs').split(/\s+/).filter(Boolean);
    if (args.length) jvm.jvmArgs = args;
    if (Object.keys(jvm).length) spec.jvm = jvm;

    const game = {};
    for (const [key, sel] of GAME_FIELDS) {
      const val = v(sel);
      if (val !== '') game[key] = val;
    }
    if (Object.keys(game).length) spec.game = game;

    const launch = {};
    const server = v('#pf-server');
    if (server) {
      launch.server = server;
      const port = v('#pf-port');
      if (port) launch.port = port;
    }
    const quickWorld = v('#pf-quickworld');
    if (quickWorld) launch.quickPlaySingleplayer = quickWorld;
    const quickServer = v('#pf-quickserver');
    if (quickServer) launch.quickPlayMultiplayer = quickServer;
    if (modal.el.querySelector('#pf-cres')?.checked) {
      launch.customResolution = true;
      const rw = v('#pf-rw');
      const rh = v('#pf-rh');
      if (rw) launch.resolutionWidth = rw;
      if (rh) launch.resolutionHeight = rh;
    }
    if (Object.keys(launch).length) spec.launch = launch;
    return spec;
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    if (this._reloadTimer) clearTimeout(this._reloadTimer);
    this._reloadTimer = null;
  }
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
