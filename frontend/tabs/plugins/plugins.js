/**
 * Plugins tab (spec 3.0 mục 84, 85).
 *
 * Liệt kê plugin trong <data>/plugins + trạng thái, quyền manifest;
 * toggle bật/tắt (reversible — disable không xoá code user, mục 76);
 * hiển thị registry: tabs/panels/commands plugin đã đăng ký.
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

export class PluginsView {
  constructor(el) {
    this.el = el;
  }

  async mount() {
    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('pl.title')}</div>
          <div class="view-sub">${t('pl.subtitle')}</div>
        </div>
        <button class="btn ghost" id="pl-refresh">${icon('refresh', 15, 'btn-icon')}${t('common.refresh')}</button>
      </div>

      <div class="card section" style="margin-top:0">
        <h3>${icon('mods', 14)} ${t('pl.listTitle')}</h3>
        <div id="pl-list" class="pl-list">${t('common.loading')}</div>
      </div>

      <div class="card section">
        <h3>${icon('sparkle', 14)} ${t('pl.registryTitle')}</h3>
        <div id="pl-registry"></div>
      </div>`;

    this.el.querySelector('#pl-refresh').addEventListener('click', () => this._load());
    await this._load();
  }

  unmount() { /* không có subscription */ }

  async _load() {
    const list = this.el.querySelector('#pl-list');
    const reg = this.el.querySelector('#pl-registry');
    try {
      const res = await call('plugins_list');
      if (!res.ok) throw new Error(res.error?.message || 'plugins_list failed');
      const plugins = res.data.plugins || [];

      if (!plugins.length) {
        list.innerHTML = `<div class="empty"><div class="big">${icon('mods', 26)}</div>
          <div class="empty-title">${t('pl.empty')}</div>
          <div class="empty-sub">${t('pl.emptyHint')}</div></div>`;
      } else {
        list.innerHTML = plugins.map((p) => this._row(p)).join('');
        list.querySelectorAll('[data-toggle]').forEach((btn) => {
          btn.addEventListener('click', () => this._toggle(btn.dataset.toggle, btn.dataset.next === 'true'));
        });
      }

      const r2 = await call('plugins_registry');
      reg.innerHTML = r2.ok ? this._registry(r2.data) : `<div class="empty-sub">${t('common.error')}</div>`;
    } catch (e) {
      handleError(e);
      list.innerHTML = `<div class="empty-sub">${t('common.error')}</div>`;
    }
  }

  _row(p) {
    const badge = p.invalid
      ? `<span class="badge offline">${t('pl.invalid')}</span>`
      : p.loaded
        ? `<span class="badge online">${t('pl.loaded')}</span>`
        : p.enabled
          ? `<span class="badge info">${t('pl.on')}</span>`
          : `<span class="badge">${t('pl.off')}</span>`;
    const perms = (p.permissions || []).map((x) => `<span class="chip">${esc(x)}</span>`).join('');
    const errLine = p.invalid ? `<div class="empty-sub">⚠ ${esc(p.error || '')}</div>` : '';
    const toggle = p.invalid ? '' : `
      <button class="btn ${p.loaded ? 'ghost' : 'primary'}" data-toggle="${esc(p.id)}"
              data-next="${p.loaded ? 'false' : 'true'}">
        ${p.loaded ? t('pl.disable') : t('pl.enable')}</button>`;
    return `
      <div class="pl-row">
        <div class="pl-row-main">
          <div class="pl-name">${esc(p.name || p.id)} <span class="pl-ver">${esc(p.version || '')}</span> ${badge}</div>
          <div class="pl-id">${esc(p.id)}</div>
          ${perms ? `<div class="pl-perms">${perms}</div>` : ''}
          ${errLine}
        </div>
        <div class="pl-row-actions">${toggle}</div>
      </div>`;
  }

  _registry(data) {
    const tabs = data.tabs || [];
    const panels = data.panels || {};
    const commands = data.commands || [];
    const panelCount = Object.values(panels).reduce((n, arr) => n + arr.length, 0);
    if (!tabs.length && !panelCount && !commands.length) {
      return `<div class="empty-sub">${t('pl.registryEmpty')}</div>`;
    }
    return `
      ${tabs.length ? `<div class="pl-reg-line">${t('pl.regTabs')}: ${tabs.map((x) => `<span class="chip">${esc(x.id)}</span>`).join('')}</div>` : ''}
      ${panelCount ? `<div class="pl-reg-line">${t('pl.regPanels')}: ${panelCount}</div>` : ''}
      ${commands.length ? `<div class="pl-reg-line">${t('pl.regCommands')}: ${commands.map((c) => `<span class="chip">${esc(c)}</span>`).join('')}</div>` : ''}`;
  }

  async _toggle(id, enable) {
    try {
      const res = await call('plugins_set_enabled', id, enable);
      if (!res.ok) throw new Error(res.error?.message || 'toggle failed');
      toastSuccess(enable ? t('pl.toastEnabled') : t('pl.toastDisabled'));
      await this._load();
    } catch (e) {
      toastError(e.message || String(e));
    }
  }
}
