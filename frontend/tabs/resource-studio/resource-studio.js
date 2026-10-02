/**
 * Resource Pack Studio tab (spec 3.0 mục 10, 33).
 *
 * Wizard đúng flow 10.2: Create → version → template → (modules từ template)
 * → Validate → Build → Install vào instance.
 * - Project cards: name, MC version, template, buildCount, actions.
 * - Detail panel: validate findings + builds history + install target.
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { confirmDialog, openModal } from '../../app/modal.js';
import { onBackendEvent } from '../../app/events.js';
import { AssetLibraryView } from './asset-library.js';
import { PackImportWizard } from './pack-import.js';
import { renderLayersPanel } from './layers-panel.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const fmtBytes = (b) => {
  if (!b || b <= 0) return '0 B';
  if (b >= 1024 ** 2) return `${(b / 1024 ** 2).toFixed(2)} MB`;
  if (b >= 1024) return `${(b / 1024).toFixed(0)} KB`;
  return `${b} B`;
};

export class ResourceStudioView {
  constructor(el) {
    this.el = el;
    this._unsubs = [];
    this._projects = [];
    this._detailId = null;
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('rs.title')}</div>
          <div class="view-sub">${t('rs.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn" id="rs-import-pack">${icon('upload', 15, 'btn-icon')}${t('zi.importPack')}</button>
          <button class="btn primary" id="rs-new">${icon('plus', 15, 'btn-icon')}${t('rs.create')}</button>
        </div>
      </div>
      <div class="tabs" id="rs-subtabs" role="tablist">
        <button class="tab active" role="tab" data-rstab="projects" aria-selected="true">${icon('layers', 14)} ${esc(t('rs.tab.projects'))}</button>
        <button class="tab" role="tab" data-rstab="assets" aria-selected="false">${icon('copy', 14)} ${esc(t('rs.tab.assets'))}</button>
      </div>
      <div id="rs-projects">
        <div id="rs-list" class="rs-list"></div>
        <div id="rs-detail"></div>
      </div>
      <div id="rs-assets" hidden></div>`;

    this.el.querySelector('#rs-new').addEventListener('click', () => this._wizard());
    // Import Pack wizard (Batch 6 — mục 13, 51, 93)
    this.el.querySelector('#rs-import-pack')?.addEventListener('click', () => this._importPack());
    // Subtab switch: projects <-> assets (mục 10.1 navigation)
    this.el.querySelector('#rs-subtabs').addEventListener('click', (e) => {
      const btn = e.target.closest('[data-rstab]');
      if (!btn) return;
      this._switchTab(btn.dataset.rstab);
    });

    // Asset Library — lazy mount lần đầu (mục 77: load core only)
    this._assetView = null;
    this._assetHost = this.el.querySelector('#rs-assets');

    this._unsubs.push(
      onBackendEvent('resource.built', () => this._refresh()),
      onBackendEvent('resource.installed', () => this._refresh()),
    );

    await this._refresh();
  }

  async _switchTab(tab) {
    const isAssets = tab === 'assets';
    this.el.querySelectorAll('[data-rstab]').forEach((b) => {
      const on = b.dataset.rstab === tab;
      b.classList.toggle('active', on);
      b.setAttribute('aria-selected', String(on));
    });
    this.el.querySelector('#rs-projects').hidden = isAssets;
    this.el.querySelector('#rs-assets').hidden = !isAssets;
    if (isAssets) {
      if (!this._assetView) {
        this._assetView = new AssetLibraryView(this._assetHost);
        await this._assetView.mount();
      } else {
        await this._assetView._refresh();
      }
    }
  }

  async _refresh() {
    const wrap = this.el.querySelector('#rs-list');
    // Loading skeleton (mục 20.4) — chỉ khi chưa có dữ liệu
    if (wrap && !this._projects.length) {
      wrap.innerHTML = `
        <div class="rs-card skel-card"><div class="row"><span class="skel skel-icon"></span>
          <div class="grow"><div class="skel skel-line"></div>
          <div class="skel skel-line short"></div></div></div>
          <div class="row"><div class="skel skel-line"></div></div></div>`;
    }
    const res = await call('resource_list');
    if (!res?.ok) { handleError(res.error); return; }
    this._projects = res.data.projects || [];
    if (!wrap) return;

    wrap.innerHTML = this._projects.length
      ? this._projects.map((p) => `
        <div class="card hoverable rs-card${p.id === this._detailId ? ' rs-active' : ''}" data-id="${esc(p.id)}">
          <div class="row" style="align-items:flex-start">
            <span class="rs-icon">${icon('layers', 20)}</span>
            <div class="grow">
              <div><b>${esc(p.name)}</b> <span class="badge">${esc(t('common.version'))} ${esc(p.minecraft?.version || '?')}</span></div>
              <div class="muted">${esc(t('rs.template.' + (p.template || 'minimal')))} · ${p.buildCount || 0} ${t('rs.builds')}</div>
            </div>
            <button class="icon-btn plain" data-act="delete" title="${esc(t('common.delete'))}">${icon('trash', 15)}</button>
          </div>
          <div class="rs-actions">
            <button class="btn sm" data-act="detail">${icon('edit', 13, 'btn-icon')}${t('rs.open')}</button>
            <button class="btn sm" data-act="validate">${icon('shieldCheck', 13, 'btn-icon')}${t('rs.validate')}</button>
            <button class="btn sm" data-act="build">${icon('database', 13, 'btn-icon')}${t('rs.build')}</button>
            <button class="btn sm primary" data-act="install">${icon('check', 13, 'btn-icon')}${t('rs.install')}</button>
          </div>
        </div>`).join('')
      : `<div class="empty"><div class="big">${icon('layers', 26)}</div>
         <div class="empty-title">${t('rs.none')}</div><div>${t('rs.noneSub')}</div></div>`;

    wrap.querySelectorAll('.rs-card').forEach((card) => {
      const id = card.dataset.id;
      card.querySelectorAll('[data-act]').forEach((btn) => {
        btn.addEventListener('click', async (e) => {
          e.stopPropagation();
          const act = btn.dataset.act;
          if (act === 'delete') return this._delete(id);
          if (act === 'detail') return this._detail(id);
          if (act === 'validate') return this._validate(id);
          if (act === 'build') return this._build(id);
          if (act === 'install') return this._install(id);
        });
      });
      card.addEventListener('click', () => this._detail(id));
    });
  }

  async _importPack() {
    const res = await call('resource_list');
    if (!res?.ok) { handleError(res.error); return; }
    const projects = res.data.projects || [];
    if (!projects.length) { toastError(t('rs.error.noProject')); return; }
    new PackImportWizard(projects, () => this._refresh()).open();
  }

  /* ---------------- Wizard (mục 10.2) ---------------- */

  async _wizard() {
    const info = await call('resource_wizard_info');
    const templates = info?.ok ? info.data.templates : [];
    // Danh sách version đã xác minh từ backend (mục 6 — không hardcode frontend)
    const versions = (info?.ok && info.data.versions?.length)
      ? info.data.versions
      : ['1.21.4', '1.21.1', '1.21', '1.20.6', '1.20.4', '1.20.1', '1.19.4', '1.18.2', '1.16.5', '1.12.2', '1.8.9'];
    const defaultVer = (info?.ok && info.data.defaultVersion) || versions[0];

    const m = openModal({
      title: t('rs.wizardTitle'),
      icon: 'layers',
      body: `
        <div class="field"><label>${t('rs.name')}</label>
          <input type="text" id="rs-wiz-name" placeholder="${esc(t('rs.namePlaceholder'))}" /></div>
        <div class="field"><label>${t('rs.version')}</label>
          <select id="rs-wiz-version">${versions.map((v) =>
            `<option value="${v}"${v === defaultVer ? ' selected' : ''}>${v}</option>`).join('')}</select></div>
        <div class="field"><label>${t('rs.templateLabel')}</label>
          <select id="rs-wiz-template">${templates.map((tp) =>
            `<option value="${esc(tp.id)}">${esc(t('rs.template.' + tp.labelKey))} — ${esc(t('rs.template.' + tp.descKey))}</option>`).join('')}</select></div>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('rs.create'), variant: 'primary', autofocus: true,
          onClick: async ({ close }) => {
            const name = m.el.querySelector('#rs-wiz-name')?.value?.trim();
            const version = m.el.querySelector('#rs-wiz-version')?.value;
            const template = m.el.querySelector('#rs-wiz-template')?.value;
            if (!name) { toastError(t('rs.error.name')); return; }
            const res = await call('resource_create', name, version, template || 'minimal');
            if (!res?.ok) { handleError(res.error); return; }
            close();
            toastSuccess(t('rs.toast.created', { name }));
            this._detailId = res.data.project.id;
            await this._refresh();
            this._detail(res.data.project.id);
          },
        },
      ],
    });
    queueMicrotask(() => m.el.querySelector('#rs-wiz-name')?.focus());
  }

  /* ---------------- Actions ---------------- */

  async _detail(id) {
    this._detailId = id;
    this.el.querySelectorAll('.rs-card').forEach((c) =>
      c.classList.toggle('rs-active', c.dataset.id === id));
    const box = this.el.querySelector('#rs-detail');
    if (!box) return;

    const [proj, builds] = await Promise.all([call('resource_get', id), call('resource_builds', id)]);
    if (!proj?.ok || !box.isConnected) return;
    const p = proj.data.project;
    const buildList = builds?.ok ? builds.data.builds : [];

    box.innerHTML = `
      <div class="card section rs-detail">
        <h3>${icon('edit', 14)} ${esc(p.name)}</h3>
        <div class="muted">${esc(t('rs.packFormat'))}: <b>${esc(String(p.packFormat || this._pf(p)))}</b> ·
          ${esc(t('rs.updated'))} ${new Date((p.updatedAt || 0) * 1000).toLocaleString()}</div>
        <h3 style="margin-top:14px">${icon('database', 14)} ${t('rs.buildHistory')}</h3>
        <div class="list">${buildList.length ? buildList.map((b) => `
          <div class="row">
            <span class="grow truncate"><b>${esc(b.file)}</b>
              <span class="muted">· ${b.files} ${t('rs.files')} · ${fmtBytes(b.bytes)}</span></span>
            <span class="muted">${new Date((b.builtAt || 0) * 1000).toLocaleString()}</span>
          </div>`).join('') : `<div class="muted" style="padding:8px 0">${t('rs.noBuilds')}</div>`}</div>
        <div id="rs-layers-panel" style="margin-top:14px"></div>
      </div>`;

    // Pack Layers panel (Batch 7 — mục 25) — chỉ khi có instance
    try {
      const inst = await call('instances_list');
      const list = inst?.ok ? inst.data.instances : [];
      if (list.length) await renderLayersPanel(
        box.querySelector('#rs-layers-panel'), list, () => this._detail(id));
    } catch { /* no-op */ }
  }

  _pf(p) {
    // packFormat không lưu trong project — hiển thị qua validate/build info.
    return '?';
  }

  async _validate(id) {
    const res = await call('resource_validate', id);
    if (!res?.ok) { handleError(res.error); return; }
    const { ok, findings, packFormat } = res.data;
    const rows = findings.length
      ? findings.map((f) => `
        <div class="row">
          <span class="badge ${f.severity === 'ERROR' ? 'offline' : 'warn'}">${esc(f.severity)}</span>
          <span class="grow truncate"><b>${esc(f.code)}</b> <span class="muted">${esc(f.path)}</span></span>
        </div>`).join('')
      : `<div class="row"><span class="badge online">${t('rs.clean')}</span>
         <span class="grow">${t('rs.noFindings')}</span></div>`;
    openModal({
      title: `${t('rs.validate')} — pack_format ${packFormat}`,
      icon: 'shieldCheck',
      tone: ok ? 'accent' : 'danger',
      wide: true,
      body: rows,
      actions: [{ label: t('common.close'), variant: 'primary' }],
    });
  }

  async _build(id) {
    const res = await call('resource_build', id);
    if (!res?.ok) { handleError(res.error); return; }
    toastSuccess(t('rs.toast.built', { file: res.data.file, size: fmtBytes(res.data.bytes) }));
    await this._refresh();
    this._detail(id);
  }

  async _install(id) {
    const instances = await call('instances_list');
    if (!instances?.ok) { handleError(instances.error); return; }
    const list = instances.data.instances || [];
    if (!list.length) { toastError(t('rs.error.noInstance')); return; }

    const m = openModal({
      title: t('rs.installTitle'),
      icon: 'check',
      body: `
        <div class="field"><label>${t('opt.instance')}</label>
          <select id="rs-inst-target">${list.map((i) =>
            `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join('')}</select></div>`,
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('rs.install'), variant: 'primary',
          onClick: async ({ close }) => {
            const target = m.el.querySelector('#rs-inst-target')?.value;
            const res = await call('resource_install', id, target);
            if (!res?.ok) { handleError(res.error); return; }
            close();
            toastSuccess(t('rs.toast.installed', { file: res.data.file }));
          },
        },
      ],
    });
  }

  async _delete(id) {
    const ok = await confirmDialog({
      title: t('rs.deleteTitle'),
      message: t('rs.deleteMsg'),
    });
    if (!ok) return;
    const res = await call('resource_delete', id);
    if (!res?.ok) { handleError(res.error); return; }
    if (this._detailId === id) {
      this._detailId = null;
      const box = this.el.querySelector('#rs-detail');
      if (box) box.innerHTML = '';
    }
    toastSuccess(t('rs.toast.deleted'));
    this._refresh();
  }

  unmount() {
    this._unsubs.forEach((off) => off());
    this._unsubs = [];
    this._assetView?.unmount?.();    // mục 11: dọn sạch view con
    this._assetView = null;
  }
}
