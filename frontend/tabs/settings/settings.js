/** Settings tab — appearance, language, performance, about (mục 153-156, 575-577). */
import { call } from '../../app/bridge.js';
import { toastSuccess } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon } from '../../app/icons.js';
import { t, setLanguage, detectSystemLanguage } from '../../i18n/i18n.js';
import { getAppearance, setAppearance, ACCENTS, THEMES, ANIMATIONS, FONT_SCALES } from '../../app/appearance.js';

const ACCENT_LABEL = {
  red: 'settings.accent.red',
  gold: 'settings.accent.gold',
  blue: 'settings.accent.blue',
  violet: 'settings.accent.violet',
  teal: 'settings.accent.teal',
  magenta: 'settings.accent.magenta',
};

const ANIM_LABEL = {
  full: 'settings.animations.full',
  reduced: 'settings.animations.reduced',
  off: 'settings.animations.off',
};

export class SettingsView {
  constructor(el) { this.el = el; }

  async mount() {
    const [res] = await Promise.all([call('settings_get')]);
    const root = res?.ok ? res.data : {};
    const ui = root.ui || {};
    const perf = root.performance || {};
    const currentLang = ui.language || 'vi';
    const detected = detectSystemLanguage();
    const look = getAppearance();

    this.el.innerHTML = `
      <div class="view-title">${t('settings.title')}</div>
      <div class="view-sub">${t('settings.subtitle')}</div>

      <div class="settings-grid">
        <!-- Appearance -->
        <div class="card">
          <h3>${icon('sparkle', 14)} ${t('settings.appearance')}</h3>
          <div class="grid-2 stagger">
            <div class="field">
              <label for="st-theme">${t('settings.theme')}</label>
              <select id="st-theme">
                ${THEMES.map((v) => `<option value="${v}" ${look.theme === v ? 'selected' : ''}>
                  ${t('settings.theme.' + v)}</option>`).join('')}
              </select>
            </div>
            <div class="field">
              <label for="st-accent">${t('settings.accent')}</label>
              <select id="st-accent">
                ${ACCENTS.map((v) => `<option value="${v}" ${look.accent === v ? 'selected' : ''}>
                  ${t(ACCENT_LABEL[v])}</option>`).join('')}
              </select>
            </div>
            <div class="field">
              <label for="st-anim">${t('settings.animations')}</label>
              <select id="st-anim">
                ${ANIMATIONS.map((v) => `<option value="${v}" ${look.animations === v ? 'selected' : ''}>
                  ${t(ANIM_LABEL[v])}</option>`).join('')}
              </select>
            </div>
            <div class="field">
              <label for="st-sidebar">${t('settings.sidebar')}</label>
              <select id="st-sidebar">
                <option value="expanded" ${look.sidebarCollapsed ? '' : 'selected'}>
                  ${t('settings.sidebar.expanded')}</option>
                <option value="collapsed" ${look.sidebarCollapsed ? 'selected' : ''}>
                  ${t('settings.sidebar.collapsed')}</option>
              </select>
            </div>
            <div class="field">
              <label for="st-fontscale">${t('settings.fontScale')}</label>
              <select id="st-fontscale">
                ${FONT_SCALES.map((v) => `<option value="${v}" ${String(look.fontScale) === v ? 'selected' : ''}>${Math.round(v * 100)}%</option>`).join('')}
              </select>
            </div>
            <div class="field">
              <label for="st-density">${t('settings.density')}</label>
              <select id="st-density">
                <option value="comfortable" ${look.density === 'comfortable' ? 'selected' : ''}>
                  ${t('settings.density.comfortable')}</option>
                <option value="compact" ${look.density === 'compact' ? 'selected' : ''}>
                  ${t('settings.density.compact')}</option>
              </select>
            </div>
          </div>
        </div>

        <!-- Language -->
        <div class="card">
          <h3>${icon('globe', 14)} ${t('settings.language')}</h3>
          <div class="field">
            <select id="st-lang">
              <option value="auto" ${currentLang === 'auto' ? 'selected' : ''}>
                ${t('settings.language.auto')} — ${detected === 'vi' ? 'Tiếng Việt' : 'English'}</option>
              <option value="vi" ${currentLang === 'vi' ? 'selected' : ''}>${t('settings.language.vi')}</option>
              <option value="en" ${currentLang === 'en' ? 'selected' : ''}>${t('settings.language.en')}</option>
            </select>
            <div class="hint">System: ${navigator.language || '?'} → ${detected}</div>
          </div>
        </div>

        <!-- Performance -->
        <div class="card">
          <h3>${icon('memory', 14)} ${t('settings.performance')}</h3>
          <div class="grid-2 stagger">
            <div class="field">
              <label for="st-ram">${t('settings.defaultRam')}</label>
              <input type="text" id="st-ram" value="${perf.defaultMaxMb || 2048}" />
            </div>
            <div class="field">
              <label for="st-preset">${t('settings.jvmPreset')}</label>
              <select id="st-preset">
                <option value="auto" ${perf.jvmPreset === 'auto' ? 'selected' : ''}>${t('settings.jvm.auto')}</option>
                <option value="balanced" ${perf.jvmPreset === 'balanced' ? 'selected' : ''}>${t('settings.jvm.balanced')}</option>
                <option value="low_memory" ${perf.jvmPreset === 'low_memory' ? 'selected' : ''}>${t('settings.jvm.lowMemory')}</option>
              </select>
            </div>
          </div>
          <div style="display:flex;justify-content:flex-end">
            <button class="btn success" id="st-save">${icon('check', 15, 'btn-icon')}${t('accounts.save')}</button>
          </div>
        </div>

        <!-- About -->
        <div class="card">
          <h3>${icon('info', 14)} ${t('settings.about')}</h3>
          <div class="kv"><span class="kv-key">App</span><span class="kv-val">Antares Launcher ${t('app.version')}</span></div>
          <div class="kv"><span class="kv-key">${t('settings.architecture')}</span><span class="kv-val">${t('settings.architectureValue')}</span></div>
          <div class="kv"><span class="kv-key">${t('settings.baseline')}</span><span class="kv-val">${t('settings.baselineValue')}</span></div>
        </div>
      </div>`;

    this._bindAppearance();
    this._bindLanguage();
    this._bindPerformance();
  }

  _bindAppearance() {
    // Áp ngay + persist (không cần reload — mục 577)
    this.el.querySelector('#st-theme')?.addEventListener('change', async (e) => {
      await setAppearance({ theme: e.target.value });
      toastSuccess(t('settings.saved'));
    });
    this.el.querySelector('#st-accent')?.addEventListener('change', async (e) => {
      await setAppearance({ accent: e.target.value });
      toastSuccess(t('settings.saved'));
    });
    this.el.querySelector('#st-anim')?.addEventListener('change', async (e) => {
      await setAppearance({ animations: e.target.value });
      toastSuccess(t('settings.saved'));
    });
    this.el.querySelector('#st-sidebar')?.addEventListener('change', async (e) => {
      await setAppearance({ sidebarCollapsed: e.target.value === 'collapsed' });
      toastSuccess(t('settings.saved'));
    });
    this.el.querySelector('#st-fontscale')?.addEventListener('change', async (e) => {
      await setAppearance({ fontScale: e.target.value });
      toastSuccess(t('settings.saved'));
    });
    this.el.querySelector('#st-density')?.addEventListener('change', async (e) => {
      await setAppearance({ density: e.target.value });
      toastSuccess(t('settings.saved'));
    });
  }

  _bindLanguage() {
    this.el.querySelector('#st-lang')?.addEventListener('change', async (e) => {
      // setLanguage -> onLanguageChange -> shell vẽ lại toàn bộ chrome + tab hiện tại
      await setLanguage(e.target.value);
    });
  }

  _bindPerformance() {
    this.el.querySelector('#st-save')?.addEventListener('click', async () => {
      const ram = parseInt(this.el.querySelector('#st-ram').value, 10) || 2048;
      const preset = this.el.querySelector('#st-preset').value;
      const res = await call('settings_update', 'performance', { defaultMaxMb: ram, jvmPreset: preset });
      if (!res.ok) { handleError(res.error); return; }
      toastSuccess(t('settings.saved'));
    });
  }

  unmount() {}
}
