/**
 * Pack Import UI (master plan v3 mục 13, 51, 93 — Batch 6).
 *
 * Wizard: chọn ZIP -> backend inspect (không ghi gì) -> summary findings
 * + entry list -> user chọn conflict policy (keep/replace/keep_both) ->
 * import -> result (imported + conflicts). Progress state machine mục 84:
 * idle -> inspecting -> ready -> importing -> success/partial/error.
 */
import { call } from '../../app/bridge.js';
import { icon } from '../../app/icons.js';
import { t } from '../../i18n/i18n.js';
import { toastSuccess, toastError } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { openModal } from '../../app/modal.js';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
}[c]));

const fmtBytes = (b) => {
  if (!b || b <= 0) return '0 B';
  if (b >= 1024 ** 2) return `${(b / 1024 ** 2).toFixed(1)} MB`;
  if (b >= 1024) return `${(b / 1024).toFixed(0)} KB`;
  return `${b} B`;
};

const POLICIES = [
  { id: 'keep', labelKey: 'zi.policy.keep' },
  { id: 'keep_both', labelKey: 'zi.policy.keepBoth' },
  { id: 'replace', labelKey: 'zi.policy.replace' },
];

export class PackImportWizard {
  /**
   * @param {string[]} projects [{id, name, minecraft.version}] — backend list
   * @param {Function} onChanged gọi sau import thành công (refresh list)
   */
  constructor(projects, onChanged) {
    this._projects = projects;
    this._onChanged = onChanged;
    this._file = null;         // {name, b64}
    this._report = null;
    this._policy = 'keep';
  }

  open() {
    this._file = null;
    this._report = null;
    this._policy = 'keep';
    this._m = openModal({
      title: t('zi.title'),
      icon: 'upload',
      wide: true,
      body: this._bodyStepPick(),
      actions: [
        { label: t('common.cancel'), variant: 'ghost', onClick: ({ close }) => close() },
        { label: t('zi.next'), variant: 'primary', autofocus: true,
          onClick: ({ close }) => this._stepInspect(close) },
      ],
    });
    this._bindPick();
  }

  /* ================= step 1 — pick file ================= */

  _bodyStepPick() {
    const projectOpts = this._projects.map((p) =>
      `<option value="${esc(p.id)}">${esc(p.name)} (${esc(p.minecraft?.version || '?')})</option>`).join('');
    return `
      <div class="field"><label>${esc(t('zi.project'))}</label>
        <select id="zi-project">${projectOpts || `<option value="">${esc(t('rs.none'))}</option>`}</select></div>
      <div class="zi-drop" id="zi-drop" tabindex="0" role="button"
           aria-label="${esc(t('zi.dropLabel'))}">
        ${icon('upload', 22)}
        <div>${esc(t('zi.dropLabel'))}</div>
        <div class="muted">${esc(t('zi.dropHint'))}</div>
      </div>
      <input type="file" id="zi-file" accept=".zip,application/zip" hidden />
      <div id="zi-picked" class="muted"></div>`;
  }

  _bindPick() {
    const drop = this._m.el.querySelector('#zi-drop');
    const file = this._m.el.querySelector('#zi-file');
    drop.addEventListener('click', () => file.click());
    drop.addEventListener('dragover', (e) => { e.preventDefault(); drop.classList.add('over'); });
    drop.addEventListener('dragleave', () => drop.classList.remove('over'));
    drop.addEventListener('drop', (e) => {
      e.preventDefault();
      drop.classList.remove('over');
      const f = e.dataTransfer?.files?.[0];
      if (f) this._setFile(f);
    });
    file.addEventListener('change', () => {
      if (file.files[0]) this._setFile(file.files[0]);
      file.value = '';
    });
  }

  _setFile(f) {
    if (!/\.zip$/i.test(f.name) && f.type !== 'application/zip') {
      toastError(t('zi.error.notZip'));
      return;
    }
    if (f.size > 64 * 1024 * 1024) {
      toastError(t('zi.error.tooLarge'));
      return;
    }
    const r = new FileReader();
    r.onload = () => {
      this._file = { name: f.name, b64: String(r.result).split(',', 2)[1] || '' };
      const box = this._m.el.querySelector('#zi-picked');
      if (box) box.innerHTML = `${icon('check', 13)} <b>${esc(f.name)}</b> (${fmtBytes(f.size)})`;
    };
    r.readAsDataURL(f);
  }

  /* ================= step 2 — inspect summary ================= */

  async _stepInspect(closePick) {
    const projectId = this._m.el.querySelector('#zi-project')?.value;
    if (!projectId) { toastError(t('rs.error.noProject')); return; }
    if (!this._file) { toastError(t('zi.error.noFile')); return; }

    const body = this._m.el.querySelector('.modal-body');
    body.innerHTML = `<div class="muted">${esc(t('zi.inspecting'))}</div>`;

    const res = await call('resource_zip_inspect', this._file.b64);
    if (!res?.ok) { handleError(res.error); return; }
    this._report = res.data;
    this._projectId = projectId;

    if (!this._report.ok) {
      // ERROR findings — chặn (mục 14: không cho import khi security fail)
      const errors = this._report.findings.filter((f) => f.severity === 'ERROR');
      body.innerHTML = `
        <div class="zi-finding zi-error">${icon('alert', 15)} ${esc(t('zi.rejected'))}</div>
        <div class="list">${errors.slice(0, 12).map((f) => `
          <div class="row"><span class="badge offline">${esc(f.code)}</span>
            <span class="grow truncate">${esc(f.path || '—')}</span>
            <span class="muted truncate">${esc(f.detail)}</span></div>`).join('')}</div>`;
      // đổi nút Next thành Close
      const btn = this._m.el.querySelector('.modal-actions .primary');
      if (btn) { btn.textContent = t('common.close'); btn.onclick = () => this._m.close(); }
      return;
    }

    body.innerHTML = `
      <div class="zi-summary">${icon('shieldCheck', 15)}
        ${esc(t('zi.summary', { files: this._report.totals.files, size: fmtBytes(this._report.totals.bytes) }))}</div>
      ${this._report.findings.length ? `<div class="zi-finding">${esc(t('zi.findings'))}</div>
        <div class="list">${this._report.findings.slice(0, 10).map((f) => `
          <div class="row"><span class="badge ${f.severity === 'ERROR' ? 'offline' : 'warn'}">${esc(f.severity)}</span>
            <span class="grow truncate">${esc(f.path)}</span>
            <span class="muted truncate">${esc(f.detail)}</span></div>`).join('')}
          ${this._report.findings.length > 10 ? `<div class="muted">+${this._report.findings.length - 10}…</div>` : ''}</div>` : ''}
      <div class="field"><label>${esc(t('zi.conflictPolicy'))}</label>
        <div class="zi-policies">
          ${POLICIES.map((p) => `
            <label class="switch"><input type="radio" name="zi-policy" value="${p.id}"
              ${p.id === this._policy ? 'checked' : ''} />
              <span class="track"></span>${esc(t(p.labelKey))}</label>`).join('')}
        </div></div>
      <div class="muted">${esc(t('zi.policyHint'))}</div>`;

    body.querySelectorAll('input[name="zi-policy"]').forEach((inp) => {
      inp.addEventListener('change', () => { this._policy = inp.value; });
    });
    // đổi nút Next thành Import
    const btn = this._m.el.querySelector('.modal-actions .primary');
    if (btn) {
      btn.textContent = t('zi.import');
      btn.onclick = () => this._stepImport();
    }
  }

  /* ================= step 3 — import result ================= */

  async _stepImport() {
    const body = this._m.el.querySelector('.modal-body');
    body.innerHTML = `<div class="muted">${esc(t('zi.importing'))}</div>`;
    const btn = this._m.el.querySelector('.modal-actions .primary');
    if (btn) btn.disabled = true;

    const res = await call('resource_zip_import', this._projectId,
      this._file.b64, this._policy, {});
    if (!res?.ok) { handleError(res.error); return; }

    const { imported, conflicts } = res.data;
    body.innerHTML = `
      <div class="zi-finding zi-ok">${icon('check', 15)}
        ${esc(t('zi.done', { count: imported.length }))}</div>
      ${conflicts?.length ? `<div class="zi-finding">${esc(t('zi.conflicts', { count: conflicts.length }))}</div>
        <div class="list">${conflicts.slice(0, 12).map((c) => `
          <div class="row"><span class="badge">${esc(t('zi.res.' + c.resolution))}</span>
            <span class="grow truncate">${esc(c.path)}</span>
            ${c.newPath ? `<span class="muted truncate">→ ${esc(c.newPath)}</span>` : ''}</div>`).join('')}</div>` : ''}
      ${imported.length ? `<div class="muted">${esc(imported.slice(0, 8).join(' · '))}${imported.length > 8 ? ` +${imported.length - 8}` : ''}</div>` : ''}`;
    if (btn) {
      btn.textContent = t('common.close');
      btn.disabled = false;
      btn.onclick = () => this._m.close();
    }
    toastSuccess(t('zi.toast.imported', { count: imported.length }));
    this._onChanged?.();
  }
}
