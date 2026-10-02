/** Accounts tab — list + Add Account (offline / ely) (mục 145, 191). */
import { call } from '../../app/bridge.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { openModal, confirmDialog } from '../../app/modal.js';

export class AccountsView {
  constructor(el) { this.el = el; }

  async mount() {
    const [res, state] = await Promise.all([call('accounts_list'), call('app_get_state')]);
    const accounts = res?.ok ? res.data.accounts : [];
    const selectedId = state?.ok ? state.data.selectedAccount : null;

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('accounts.title')}</div>
          <div class="view-sub">${t('accounts.subtitle')}</div>
        </div>
        <div class="page-actions">
          <button class="btn primary" id="acc-add">${icon('plus', 15, 'btn-icon')}${t('accounts.add')}</button>
        </div>
      </div>

      <div class="card stagger">
        ${accounts.length === 0
          ? `<div class="empty">
               <div class="big">${icon('accounts', 26)}</div>
               <div class="empty-title">${t('accounts.none')}</div>
             </div>`
          : accounts.map((a) => {
            const active = a.id === selectedId;
            return `
              <div class="row">
                <span class="avatar ${a.type === 'ely' ? 'accent' : ''}">${esc((a.displayName || '?')[0])}</span>
                <span class="grow">
                  <div>${esc(a.displayName)}</div>
                  <div class="muted">${esc(a.type)}</div>
                </span>
                ${active ? `<span class="badge online">${icon('check', 12)} ${t('accounts.selected')}</span>` : ''}
                <button class="btn ghost sm" data-select="${esc(a.id)}" ${active ? 'disabled' : ''}>${t('accounts.use')}</button>
                <button class="btn danger-ghost sm" data-del="${esc(a.id)}" aria-label="${esc(t('common.delete'))}" data-tip="${esc(t('common.delete'))}">${icon('trash', 15)}</button>
              </div>`;
          }).join('')}
      </div>`;

    this.el.querySelector('#acc-add')?.addEventListener('click', () => this._dialog());
    this.el.querySelectorAll('[data-select]').forEach((b) =>
      b.addEventListener('click', async () => {
        const res2 = await call('accounts_select', b.dataset.select);
        if (!res2.ok) { handleError(res2.error); return; }
        toastSuccess(t('accounts.selected'));
        this.mount();
      }));
    this.el.querySelectorAll('[data-del]').forEach((b) =>
      b.addEventListener('click', async () => {
        const ok = await confirmDialog({
          title: t('accounts.delete'),
          message: t('accounts.confirmDelete'),
          confirmText: t('common.delete'),
        });
        if (!ok) return;
        const r = await call('accounts_remove', b.dataset.del);
        if (!r.ok) { handleError(r.error); return; }
        this.mount();
      }));
  }

  _dialog() {
    const modal = openModal({
      title: t('accounts.addTitle'),
      icon: 'accounts',
      tone: 'accent',
      body: `
        <div class="field">
          <label for="acc-type">${t('accounts.type')}</label>
          <select id="acc-type">
            <option value="offline">${t('accounts.type.offline')}</option>
            <option value="ely">${t('accounts.type.ely')}</option>
          </select>
        </div>
        <div class="field">
          <label for="acc-user">${t('accounts.username')}</label>
          <input type="text" id="acc-user" placeholder="${esc(t('accounts.username'))}" autocomplete="off" />
        </div>
        <div class="field hidden" id="acc-pass-row">
          <label for="acc-pass">${t('accounts.password')}</label>
          <input type="password" id="acc-pass" placeholder="${esc(t('accounts.passwordPlaceholder'))}" autocomplete="off" />
        </div>`,
      actions: [
        { label: t('accounts.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        {
          label: t('accounts.save'),
          variant: 'success',
          icon: 'check',
          onClick: async ({ close }) => {
            const type = modal.el.querySelector('#acc-type').value;
            const user = modal.el.querySelector('#acc-user').value.trim();
            if (!user) { toastError(t('accounts.error.username')); return; }
            const res = type === 'ely'
              ? await call('accounts_login_ely', user, modal.el.querySelector('#acc-pass').value)
              : await call('accounts_add_offline', user);
            if (!res.ok) { handleError(res.error); return; }
            toastSuccess(t('accounts.added'));
            close();
            this.mount();
          },
        },
      ],
    });

    modal.el.querySelector('#acc-type')?.addEventListener('change', (e) => {
      modal.el.querySelector('#acc-pass-row')?.classList.toggle('hidden', e.target.value !== 'ely');
    });
    requestAnimationFrame(() => modal.el.querySelector('#acc-user')?.focus());
  }

  unmount() {}
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}
