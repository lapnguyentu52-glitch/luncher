/** Settings tab PREMIUM — appearance, language, performance, about (mục 153-156, 575-577).
 *  Giữ nguyên toàn bộ hành vi/IPC cũ (settings_get, settings_update, setAppearance, setLanguage).
 *  Đổi UI: <select> → segmented / swatch màu / slider RAM, header card bằng iconBadge.
 *  Cần thêm settings.premium.css (xem file kèm theo). */
import { call } from '../../app/bridge.js';
import { toastSuccess } from '../../app/toast.js';
import { handleError } from '../../app/errors.js';
import { icon, iconBadge } from '../../app/icons.js';
import { t, setLanguage, detectSystemLanguage } from '../../i18n/i18n.js';
import { getAppearance, setAppearance, ACCENTS, THEMES, ANIMATIONS, FONT_SCALES } from '../../app/appearance.js';

const ACCENT_LABEL = {
  red: 'settings.accent.red',
  gold: 'settings.accent.gold',
  blue: 'settings.accent.blue',
  violet: 'settings.accent.violet',
  teal: 'settings.accent.teal',
  magenta: 'settings.accent.magenta',
  orange: 'settings.accent.orange',
  emerald: 'settings.accent.emerald',
};

/** Màu swatch (khớp accent-gradient trong tokens.css). Accent lạ → xám trung tính. */
const ACCENT_SWATCH = {
  red: ['#ff8566', '#e8381f'],
  gold: ['#ffe08a', '#d69f14'],
  blue: ['#86b6ff', '#2a5fd0'],
  violet: ['#c8a3ff', '#6d34d6'],
  teal: ['#6ce6da', '#0d8c80'],
  magenta: ['#ee9aee', '#9c1f9e'],
  orange: ['#ffb066', '#cc5a0a'],
  emerald: ['#6ce8aa', '#0f8f52'],
};

const ANIM_LABEL = {
  full: 'settings.animations.full',
  reduced: 'settings.animations.reduced',
  off: 'settings.animations.off',
};

const THEME_ICON = { dark: 'moon', light: 'sun' };
const ANIM_ICON = { full: 'sparkle', reduced: 'pause', off: 'xCircle' };

const RAM_MIN = 1024;
const RAM_MAX = 16384;
const RAM_STEP = 256;

/** i18n an toàn: key thiếu (accent mới) → dùng nhãn dự phòng thay vì lộ nguyên key. */
function tOr(key, fallback) {
  const v = key ? t(key) : '';
  return !v || v === key ? fallback : v;
}

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

/** Segmented control: options = [{ value, label, icon? }] */
function seg(id, options, current) {
  return `<div class="seg set-seg" id="${id}" role="radiogroup" data-seg>
    ${options.map((o) => {
      const on = String(o.value) === String(current);
      return `<button type="button" role="radio" aria-checked="${on}" data-value="${esc(o.value)}"
        class="${on ? 'active' : ''}">${o.icon ? icon(o.icon, 14) : ''}<span>${esc(o.label)}</span></button>`;
    }).join('')}
  </div>`;
}

function row(label, control, hint = '') {
  return `<div class="set-row">
    <div class="set-label"><b>${label}</b>${hint ? `<span class="muted">${hint}</span>` : ''}</div>
    <div class="set-control">${control}</div>
  </div>`;
}

function cardHead(iconName, tone, title) {
  return `<div class="set-card-head">${iconBadge(iconName, 36, tone)}<h3>${title}</h3></div>`;
}

export class SettingsView {
  constructor(el) { this.el = el; this._ac = null; }

  async mount() {
    const [res] = await Promise.all([call('settings_get')]);
    const root = res?.ok ? res.data : {};
    const ui = root.ui || {};
    const perf = root.performance || {};
    const currentLang = ui.language || 'vi';
    const detected = detectSystemLanguage();
    const look = getAppearance();
    const ram = Number(perf.defaultMaxMb) || 2048;
    const preset = perf.jvmPreset || 'auto';

    const themeOpts = THEMES.map((v) => ({ value: v, label: t('settings.theme.' + v), icon: THEME_ICON[v] || 'monitor' }));
    const animOpts = ANIMATIONS.map((v) => ({ value: v, label: t(ANIM_LABEL[v]), icon: ANIM_ICON[v] }));
    const sidebarOpts = [
      { value: 'expanded', label: t('settings.sidebar.expanded'), icon: 'panelLeft' },
      { value: 'collapsed', label: t('settings.sidebar.collapsed'), icon: 'panelLeft' },
    ];
    const fontOpts = FONT_SCALES.map((v) => ({ value: String(v), label: `${Math.round(v * 100)}%` }));
    const densityOpts = [
      { value: 'comfortable', label: t('settings.density.comfortable') },
      { value: 'compact', label: t('settings.density.compact') },
    ];
    const langOpts = [
      { value: 'auto', label: `${t('settings.language.auto')} · ${detected === 'vi' ? 'Tiếng Việt' : 'English'}`, icon: 'sparkle' },
      { value: 'vi', label: t('settings.language.vi') },
      { value: 'en', label: t('settings.language.en') },
    ];
    const presetOpts = [
      { value: 'auto', label: t('settings.jvm.auto'), icon: 'zap' },
      { value: 'balanced', label: t('settings.jvm.balanced'), icon: 'sliders' },
      { value: 'low_memory', label: t('settings.jvm.lowMemory'), icon: 'memory' },
    ];

    const swatches = ACCENTS.map((v) => {
      const [c1, c2] = ACCENT_SWATCH[v] || ['#8a92a3', '#5f6677'];
      const on = look.accent === v;
      const name = tOr(ACCENT_LABEL[v], v.charAt(0).toUpperCase() + v.slice(1));
      return `<button type="button" class="swatch ${on ? 'active' : ''}" role="radio" aria-checked="${on}"
        data-value="${esc(v)}" title="${esc(name)}" aria-label="${esc(name)}"
        style="--sw1:${c1};--sw2:${c2}">${icon('check', 14)}</button>`;
    }).join('');

    this.el.innerHTML = `
      <div class="page-head">
        <div class="page-head-text">
          <div class="view-title">${t('settings.title')}</div>
          <div class="view-sub">${t('settings.subtitle')}</div>
        </div>
      </div>

      <div class="settings-grid set-layout stagger">
        <!-- Appearance -->
        <div class="card">
          ${cardHead('brush', 'violet', t('settings.appearance'))}
          ${row(t('settings.theme'), seg('st-theme', themeOpts, look.theme))}
          ${row(t('settings.accent'), `<div class="swatches" id="st-accent" role="radiogroup" data-swatches>${swatches}</div>`)}
          ${row(t('settings.animations'), seg('st-anim', animOpts, look.animations))}
          ${row(t('settings.sidebar'), seg('st-sidebar', sidebarOpts, look.sidebarCollapsed ? 'collapsed' : 'expanded'))}
          ${row(t('settings.fontScale'), seg('st-fontscale', fontOpts, String(look.fontScale)))}
          ${row(t('settings.density'), seg('st-density', densityOpts, look.density))}
        </div>

        <!-- Language -->
        <div class="card">
          ${cardHead('globe', 'blue', t('settings.language'))}
          ${row(t('settings.language'), seg('st-lang', langOpts, currentLang),
            `System: ${esc(navigator.language || '?')} → ${esc(detected)}`)}
        </div>

        <!-- Performance -->
        <div class="card">
          ${cardHead('memory', 'emerald', t('settings.performance'))}
          ${row(t('settings.defaultRam'), `
            <div class="ram-box">
              <input type="range" id="st-ram-range" min="${RAM_MIN}" max="${RAM_MAX}" step="${RAM_STEP}"
                value="${Math.min(Math.max(ram, RAM_MIN), RAM_MAX)}" aria-label="${t('settings.defaultRam')}" />
              <div class="ram-input">
                <input type="number" id="st-ram" min="256" step="${RAM_STEP}" value="${ram}" />
                <span class="muted">MB</span>
                <span class="badge accent num" id="st-ram-gb">${(ram / 1024).toFixed(1)} GB</span>
              </div>
            </div>`)}
          ${row(t('settings.jvmPreset'), seg('st-preset', presetOpts, preset))}
          <div class="set-foot">
            <button class="btn success pill" id="st-save">${icon('check', 15, 'btn-icon')}${t('accounts.save')}</button>
          </div>
        </div>

        <!-- About -->
        <div class="card">
          ${cardHead('info', 'slate', t('settings.about'))}
          <div class="kv"><span class="kv-key">App</span><span class="kv-val">Antares Launcher <span class="badge accent">${t('app.version')}</span></span></div>
          <div class="kv"><span class="kv-key">${t('settings.architecture')}</span><span class="kv-val">${t('settings.architectureValue')}</span></div>
          <div class="kv"><span class="kv-key">${t('settings.baseline')}</span><span class="kv-val">${t('settings.baselineValue')}</span></div>
        </div>
      </div>`;

    this._ac = new AbortController();
    this._syncRamFill();
    this._bindAppearance();
    this._bindLanguage();
    this._bindPerformance();
  }

  /** Bắt sự kiện click trên 1 nhóm seg/swatch, đánh dấu active rồi gọi handler(value). */
  _bindGroup(selector, handler) {
    const group = this.el.querySelector(selector);
    if (!group) return;
    group.addEventListener('click', async (e) => {
      const btn = e.target.closest('button[data-value]');
      if (!btn || btn.classList.contains('active')) return;
      group.querySelectorAll('button[data-value]').forEach((b) => {
        const on = b === btn;
        b.classList.toggle('active', on);
        b.setAttribute('aria-checked', String(on));
      });
      await handler(btn.dataset.value);
    }, { signal: this._ac.signal });
  }

  _bindAppearance() {
    // Áp ngay + persist (không cần reload — mục 577)
    const apply = (patch) => async (v) => {
      await setAppearance(typeof patch === 'function' ? patch(v) : { [patch]: v });
      toastSuccess(t('settings.saved'));
    };
    this._bindGroup('#st-theme', apply('theme'));
    this._bindGroup('#st-accent', apply('accent'));
    this._bindGroup('#st-anim', apply('animations'));
    this._bindGroup('#st-sidebar', apply((v) => ({ sidebarCollapsed: v === 'collapsed' })));
    this._bindGroup('#st-fontscale', apply('fontScale'));
    this._bindGroup('#st-density', apply('density'));
  }

  _bindLanguage() {
    // setLanguage -> onLanguageChange -> shell vẽ lại toàn bộ chrome + tab hiện tại
    this._bindGroup('#st-lang', async (v) => { await setLanguage(v); });
  }

  /** Tô phần đã chọn của slider RAM (biến CSS --fill, 0-100%). */
  _syncRamFill() {
    const range = this.el.querySelector('#st-ram-range');
    if (!range) return;
    const pct = ((range.value - RAM_MIN) / (RAM_MAX - RAM_MIN)) * 100;
    range.style.setProperty('--fill', `${Math.min(Math.max(pct, 0), 100)}%`);
  }

  _bindPerformance() {
    const range = this.el.querySelector('#st-ram-range');
    const input = this.el.querySelector('#st-ram');
    const gb = this.el.querySelector('#st-ram-gb');
    const sig = { signal: this._ac.signal };

    const showGb = () => { gb.textContent = `${((parseInt(input.value, 10) || 0) / 1024).toFixed(1)} GB`; };
    range?.addEventListener('input', () => { input.value = range.value; showGb(); this._syncRamFill(); }, sig);
    input?.addEventListener('input', () => {
      const v = parseInt(input.value, 10);
      if (Number.isFinite(v)) { range.value = Math.min(Math.max(v, RAM_MIN), RAM_MAX); this._syncRamFill(); }
      showGb();
    }, sig);

    this._bindGroup('#st-preset', async () => { /* chỉ đổi lựa chọn, lưu khi bấm Save */ });

    const save = this.el.querySelector('#st-save');
    save?.addEventListener('click', async () => {
      const ramMb = parseInt(input.value, 10) || 2048;
      const preset = this.el.querySelector('#st-preset button.active')?.dataset.value || 'auto';
      save.disabled = true;
      try {
        const res = await call('settings_update', 'performance', { defaultMaxMb: ramMb, jvmPreset: preset });
        if (!res.ok) { handleError(res.error); return; }
        toastSuccess(t('settings.saved'));
      } finally {
        save.disabled = false;
      }
    }, sig);
  }

  unmount() { this._ac?.abort(); this._ac = null; }
}