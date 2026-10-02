<script setup lang="ts">
import { ref, watch } from 'vue'

import AppSelect from '@/app/components/AppSelect.vue'
import { applyTheme, useSettingsStore } from '@/stores/settings.store'
import { locale, setLocale, t } from '@/shared/i18n'

const settings = useSettingsStore()
const bgFileInput = ref<HTMLInputElement | null>(null)
const bgError = ref('')

// Mọi thay đổi theme → CSS vars + persist
watch(
  () => [
    settings.accent,
    settings.customAccent,
    settings.background,
    settings.customBgColor,
    settings.bgImageData,
    settings.bgDim,
    settings.glassBlur,
    settings.glassOpacity,
    settings.sidebarWidth,
    settings.reducedTransparency,
  ],
  () => {
    applyTheme(settings.$state)
    settings.persist()
  },
)

const densityOptions = [
  { value: 'comfortable', label: 'Comfortable' },
  { value: 'compact', label: 'Compact' },
]

const accentSwatches: Array<{ id: 'red' | 'blue' | 'green' | 'violet' | 'amber'; hex: string }> = [
  { id: 'red', hex: '#ff5c47' },
  { id: 'blue', hex: '#5b9dff' },
  { id: 'green', hex: '#50d890' },
  { id: 'violet', hex: '#9e7aff' },
  { id: 'amber', hex: '#ffc857' },
]

const backgroundOptions = [
  { value: 'aurora', label: 'aurora' },
  { value: 'deep', label: 'deep' },
  { value: 'plain', label: 'plain' },
  { value: 'custom', label: 'custom' },
  { value: 'image', label: 'image' },
]

const languageOptions = [
  { value: 'vi', label: 'Tiếng Việt' },
  { value: 'en', label: 'English' },
]

function onLanguage(value: string): void {
  setLocale(value === 'en' ? 'en' : 'vi')
  settings.persist()
}

async function onBgFile(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  bgError.value = ''
  if (!file) return
  const ok = await settings.setBgImage(file)
  if (!ok) bgError.value = file.type.startsWith('image/') ? 'Ảnh > 2MB' : 'Chỉ nhận ảnh PNG/JPG'
}

function onBgLabel(key: string): string {
  return t(`settings.bg_${key}`)
}

/** Phần trăm đã kéo của slider → tô màu track (--pct). */
function pct(value: number, min: number, max: number): string {
  return `${((value - min) / (max - min)) * 100}%`
}
</script>

<template>
  <section class="settings">
    <h1 class="settings__title">
      {{ t('settings.title') }}
    </h1>

    <!-- Language -->
    <div
      class="settings__section"
      style="--i: 0"
    >
      <h2 class="settings__heading">
        {{ t('settings.language') }}
      </h2>
      <AppSelect
        :model-value="locale"
        :options="languageOptions"
        :aria-label="t('settings.language')"
        class="settings__select"
        @update:model-value="onLanguage"
      />
    </div>

    <!-- Accent -->
    <div
      class="settings__section"
      style="--i: 1"
    >
      <h2 class="settings__heading">
        {{ t('settings.accent') }}
      </h2>
      <div class="settings__swatches">
        <button
          v-for="sw in accentSwatches"
          :key="sw.id"
          class="swatch"
          :class="{ 'swatch--on': settings.accent === sw.id }"
          type="button"
          :title="sw.hex"
          :aria-label="sw.id"
          :aria-pressed="settings.accent === sw.id"
          @click="settings.accent = sw.id"
        >
          <span
            class="swatch__dot"
            :style="{ background: sw.hex, boxShadow: `0 0 10px ${sw.hex}66` }"
          >
            <svg
              class="swatch__check"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="3"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            ><path d="M5.5 12.5l4.2 4.2L18.5 8" /></svg>
          </span>
        </button>

        <!-- Custom: input phủ kín nhãn → bấm là mở bảng chọn màu -->
        <label
          class="swatch swatch--custom"
          :class="{ 'swatch--on': settings.accent === 'custom' }"
          :title="t('settings.accent_custom')"
        >
          <input
            v-model="settings.customAccent"
            type="color"
            class="swatch__color-input"
            :aria-label="t('settings.accent_custom')"
            @input="settings.accent = 'custom'"
            @click="settings.accent = 'custom'"
          >
          <span
            class="swatch__dot swatch__dot--custom"
            :style="settings.accent === 'custom'
              ? { background: settings.customAccent, boxShadow: `0 0 10px ${settings.customAccent}66` }
              : undefined"
          >
            <svg
              v-if="settings.accent !== 'custom'"
              class="swatch__plus"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.6"
              stroke-linecap="round"
              aria-hidden="true"
            ><path d="M12 6v12M6 12h12" /></svg>
            <svg
              v-else
              class="swatch__check"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="3"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            ><path d="M5.5 12.5l4.2 4.2L18.5 8" /></svg>
          </span>
        </label>
      </div>
      <Transition name="note">
        <p
          v-if="settings.accent === 'custom'"
          class="settings__note"
        >
          {{ t('settings.accent_note') }}
        </p>
      </Transition>
    </div>

    <!-- Background + custom sâu -->
    <div
      class="settings__section"
      style="--i: 2"
    >
      <h2 class="settings__heading">
        {{ t('settings.background') }}
      </h2>

      <!-- Ô preview thay cho dropdown: thấy ngay nền trông thế nào -->
      <div
        class="bg-tiles"
        role="group"
        :aria-label="t('settings.background')"
      >
        <button
          v-for="o in backgroundOptions"
          :key="o.value"
          class="bg-tile"
          :class="{ 'bg-tile--on': settings.background === o.value }"
          type="button"
          :aria-pressed="settings.background === o.value"
          @click="settings.background = o.value as typeof settings.background"
        >
          <span
            class="bg-tile__preview"
            :class="`bg-tile__preview--${o.value}`"
            :style="o.value === 'custom'
              ? { background: settings.customBgColor }
              : o.value === 'image' && settings.bgImageData
                ? { backgroundImage: `url(${settings.bgImageData})` }
                : undefined"
          />
          <span class="bg-tile__label">{{ onBgLabel(o.value) }}</span>
        </button>
      </div>

      <!-- Khối tuỳ chỉnh mở rộng mượt -->
      <Transition name="expand">
        <div
          v-if="settings.background === 'custom' || settings.background === 'image'"
          class="expand"
        >
          <div class="expand__inner">
            <!-- Custom color -->
            <div
              v-if="settings.background === 'custom'"
              class="settings__bg-row"
            >
              <span>Màu chủ đạo</span>
              <input
                v-model="settings.customBgColor"
                type="color"
                class="settings__color"
                :aria-label="t('settings.bg_color_aria')"
              >
            </div>

            <!-- Ảnh nền PNG/JPG -->
            <template v-if="settings.background === 'image'">
              <div class="settings__bg-row">
                <span>{{ t('settings.bg_image') }}</span>
                <button
                  class="settings__btn"
                  type="button"
                  @click="bgFileInput?.click()"
                >
                  Chọn ảnh…
                </button>
                <input
                  ref="bgFileInput"
                  type="file"
                  accept="image/png,image/jpeg"
                  hidden
                  @change="onBgFile"
                >
              </div>
              <p class="settings__note">
                {{ t('settings.bg_image_hint') }}
              </p>
              <div class="settings__bg-row">
                <span>{{ t('settings.bg_dim') }}</span>
                <input
                  v-model.number="settings.bgDim"
                  type="range"
                  min="0"
                  max="0.85"
                  step="0.05"
                  class="settings__range"
                  :style="{ '--pct': pct(settings.bgDim, 0, 0.85) }"
                >
                <code class="settings__value">{{ Math.round(settings.bgDim * 100) }}%</code>
              </div>
              <button
                class="settings__btn settings__btn--danger"
                type="button"
                @click="settings.clearBgImage()"
              >
                {{ t('settings.bg_image_clear') }}
              </button>
            </template>
          </div>
        </div>
      </Transition>

      <Transition name="note">
        <p
          v-if="bgError"
          class="settings__note settings__note--error"
          role="alert"
        >
          {{ bgError }}
        </p>
      </Transition>
    </div>

    <!-- Glass — tùy chỉnh trong suốt -->
    <div
      class="settings__section"
      style="--i: 3"
    >
      <h2 class="settings__heading">
        {{ t('settings.glass') }}
      </h2>
      <div class="settings__bg-row">
        <span>{{ t('settings.glass_blur') }}</span>
        <input
          v-model.number="settings.glassBlur"
          type="range"
          min="0"
          max="24"
          step="1"
          class="settings__range"
          :style="{ '--pct': pct(settings.glassBlur, 0, 24) }"
        >
        <code class="settings__value">{{ settings.glassBlur }}px</code>
      </div>
      <div class="settings__bg-row">
        <span>{{ t('settings.glass_opacity') }}</span>
        <input
          v-model.number="settings.glassOpacity"
          type="range"
          min="0.4"
          max="0.95"
          step="0.02"
          class="settings__range"
          :style="{ '--pct': pct(settings.glassOpacity, 0.4, 0.95) }"
        >
        <code class="settings__value">{{ Math.round(settings.glassOpacity * 100) }}%</code>
      </div>
    </div>

    <!-- Sidebar -->
    <div
      class="settings__section"
      style="--i: 4"
    >
      <h2 class="settings__heading">
        {{ t('settings.sidebar') }}
      </h2>
      <div class="settings__bg-row">
        <span>{{ t('settings.sidebar_width') }}</span>
        <input
          v-model.number="settings.sidebarWidth"
          type="range"
          min="200"
          max="320"
          step="4"
          class="settings__range"
          :style="{ '--pct': pct(settings.sidebarWidth, 200, 320) }"
        >
        <code class="settings__value">{{ settings.sidebarWidth }}px</code>
      </div>
    </div>

    <!-- Appearance -->
    <div
      class="settings__section"
      style="--i: 5"
    >
      <h2 class="settings__heading">
        {{ t('settings.title') }} · {{ t('settings.density') }}
      </h2>
      <label class="settings__row">
        <span>{{ t('settings.density') }}</span>
        <AppSelect
          :model-value="settings.density"
          :options="densityOptions.map((o) => ({ value: o.value, label: t(`settings.density_${o.value}`) }))"
          :aria-label="t('settings.density')"
          class="settings__select settings__select--slim"
          @update:model-value="(v: string) => { settings.density = v as 'comfortable' | 'compact'; settings.persist() }"
        />
      </label>
      <label class="settings__row settings__row--click">
        <span>{{ t('settings.reduce_motion') }}</span>
        <input
          v-model="settings.reducedMotion"
          type="checkbox"
          class="toggle"
          role="switch"
        >
      </label>
      <label class="settings__row settings__row--click">
        <span>{{ t('settings.reduce_transparency') }}</span>
        <input
          v-model="settings.reducedTransparency"
          type="checkbox"
          class="toggle"
          role="switch"
        >
      </label>
    </div>
  </section>
</template>

<style scoped>
.settings {
  --ease-out: cubic-bezier(0.22, 1, 0.36, 1);
  --spring: cubic-bezier(0.34, 1.56, 0.64, 1);

  max-width: 680px;
}
.settings__title {
  margin: 0 0 18px;
  font-size: 22px;
  font-weight: 800;
  letter-spacing: 0.02em;
  background: linear-gradient(180deg, var(--text-1) 30%, var(--text-2));
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}
.settings__section {
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 18px 20px;
  margin-bottom: 16px;
  animation: section-in 560ms var(--ease-out) both;
  animation-delay: calc(var(--i, 0) * 70ms);
}
@keyframes section-in {
  from {
    opacity: 0;
    transform: translateY(14px);
  }
}
.settings__heading {
  margin: 0 0 14px;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--text-3);
}

/* ───────── Swatch màu nhấn ───────── */
.settings__swatches {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}
.swatch {
  position: relative;
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: 12px;
  cursor: pointer;
  outline: none;
  box-shadow:
    3px 3px 7px var(--neu-dark),
    -3px -3px 7px var(--neu-light);
  transition:
    box-shadow var(--motion-fast) ease,
    transform 260ms var(--spring);
}
.swatch:hover {
  transform: translateY(-2px);
}
.swatch:active {
  transform: scale(0.94);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.swatch:focus-visible,
.swatch:focus-within {
  box-shadow:
    3px 3px 7px var(--neu-dark),
    -3px -3px 7px var(--neu-light),
    0 0 0 2px var(--accent-glow);
}
.swatch--on {
  box-shadow:
    3px 3px 7px var(--neu-dark),
    -3px -3px 7px var(--neu-light),
    0 0 0 2px var(--accent-glow),
    0 0 16px var(--accent-glow);
}
.swatch__dot {
  position: relative;
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  transition: transform 300ms var(--spring);
}
.swatch:hover .swatch__dot {
  transform: scale(1.12);
}
.swatch--on .swatch__dot {
  transform: scale(1.18);
}
.swatch__check,
.swatch__plus {
  width: 12px;
  height: 12px;
  color: #fff;
  filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.45));
}
.swatch__check {
  opacity: 0;
  transform: scale(0.4);
  transition: opacity 160ms ease, transform 320ms var(--spring);
}
.swatch--on .swatch__check {
  opacity: 1;
  transform: scale(1);
}
.swatch__dot--custom {
  background: conic-gradient(from 0deg, #ff5c47, #ffc857, #50d890, #5b9dff, #9e7aff, #ff5c47);
}
.swatch__color-input {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  opacity: 0;
  cursor: pointer;
}

/* ───────── Ô chọn nền ───────── */
.bg-tiles {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 10px;
}
.bg-tile {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 7px 7px 9px;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-md);
  color: var(--text-3);
  font-size: 11px;
  cursor: pointer;
  outline: none;
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  transition:
    transform 260ms var(--spring),
    box-shadow var(--motion-fast) ease,
    color var(--motion-fast) ease,
    border-color var(--motion-fast) ease;
}
.bg-tile:hover {
  transform: translateY(-2px);
  color: var(--text-1);
}
.bg-tile:active {
  transform: scale(0.97);
}
.bg-tile:focus-visible {
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light),
    0 0 0 2px var(--accent-glow);
}
.bg-tile--on {
  color: var(--text-1);
  font-weight: 600;
  border-color: rgba(255, 92, 71, 0.4);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light),
    0 0 14px var(--accent-glow);
}
.bg-tile__preview {
  display: block;
  height: 44px;
  border-radius: var(--radius-sm);
  background-size: cover;
  background-position: center;
  box-shadow:
    inset 0 0 0 1px rgba(255, 255, 255, 0.06),
    inset 2px 2px 6px rgba(0, 0, 0, 0.4);
}
.bg-tile__preview--aurora {
  background:
    radial-gradient(circle at 20% 30%, rgba(255, 92, 71, 0.6), transparent 55%),
    radial-gradient(circle at 80% 75%, rgba(91, 157, 255, 0.55), transparent 55%),
    #12141b;
}
.bg-tile__preview--deep {
  background: linear-gradient(160deg, #14171f, #07080c);
}
.bg-tile__preview--plain {
  background: #1b1f29;
}
.bg-tile__preview--image {
  background:
    repeating-linear-gradient(135deg, rgba(255, 255, 255, 0.05) 0 6px, transparent 6px 12px),
    #171a22;
}
.bg-tile__label {
  text-align: center;
  text-transform: capitalize;
}

/* ───────── Khối mở rộng (grid-rows 0fr → 1fr) ───────── */
.expand {
  display: grid;
  grid-template-rows: 1fr;
}
.expand__inner {
  overflow: hidden;
  min-height: 0;
}
.expand-enter-active,
.expand-leave-active {
  transition: grid-template-rows 360ms var(--ease-out), opacity 260ms ease;
}
.expand-enter-from,
.expand-leave-to {
  grid-template-rows: 0fr;
  opacity: 0;
}

/* ───────── Ghi chú ───────── */
.settings__note {
  margin: 10px 0 0;
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-3);
}
.settings__note--error {
  color: var(--danger);
}
.note-enter-active {
  transition: opacity 240ms ease, transform 340ms var(--spring);
}
.note-leave-active {
  transition: opacity 120ms ease;
}
.note-enter-from {
  opacity: 0;
  transform: translateY(-5px);
}
.note-leave-to {
  opacity: 0;
}

/* ───────── Hàng + slider ───────── */
.settings__bg-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  margin-top: 14px;
  font-size: 13px;
  color: var(--text-2);
}
.settings__bg-row > span:first-child {
  min-width: 0;
}
.settings__color {
  width: 44px;
  height: 30px;
  padding: 3px;
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 9px;
  background: var(--neu-surface);
  cursor: pointer;
  box-shadow:
    3px 3px 7px var(--neu-dark),
    -3px -3px 7px var(--neu-light);
  transition: transform 220ms var(--spring);
}
.settings__color:hover {
  transform: scale(1.06);
}
.settings__color::-webkit-color-swatch-wrapper {
  padding: 0;
}
.settings__color::-webkit-color-swatch {
  border: none;
  border-radius: 6px;
}
.settings__color::-moz-color-swatch {
  border: none;
  border-radius: 6px;
}

.settings__range {
  --pct: 0%;
  -webkit-appearance: none;
  appearance: none;
  flex: 1;
  max-width: 260px;
  height: 22px;
  background: transparent;
  cursor: pointer;
  outline: none;
}
.settings__range::-webkit-slider-runnable-track {
  height: 6px;
  border-radius: 999px;
  background:
    linear-gradient(90deg, var(--accent) var(--pct), transparent var(--pct)),
    var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 4px var(--neu-dark-strong),
    inset -1px -1px 3px var(--neu-light);
}
.settings__range::-moz-range-track {
  height: 6px;
  border-radius: 999px;
  background:
    linear-gradient(90deg, var(--accent) var(--pct), transparent var(--pct)),
    var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 4px var(--neu-dark-strong),
    inset -1px -1px 3px var(--neu-light);
}
.settings__range::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 18px;
  height: 18px;
  margin-top: -6px;
  border-radius: 50%;
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: var(--neu-surface);
  box-shadow:
    2px 2px 6px var(--neu-dark-strong),
    -1px -1px 4px var(--neu-light),
    0 0 0 3px transparent;
  transition: transform 220ms var(--spring), box-shadow var(--motion-fast) ease;
}
.settings__range::-moz-range-thumb {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: var(--neu-surface);
  box-shadow:
    2px 2px 6px var(--neu-dark-strong),
    -1px -1px 4px var(--neu-light);
  transition: transform 220ms var(--spring);
}
.settings__range:hover::-webkit-slider-thumb {
  transform: scale(1.15);
}
.settings__range:active::-webkit-slider-thumb {
  transform: scale(0.95);
}
.settings__range:focus-visible::-webkit-slider-thumb {
  box-shadow:
    2px 2px 6px var(--neu-dark-strong),
    -1px -1px 4px var(--neu-light),
    0 0 0 3px var(--accent-glow);
}
.settings__range:hover::-moz-range-thumb {
  transform: scale(1.15);
}

.settings__value {
  min-width: 52px;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 11px;
  color: var(--text-1);
  text-align: center;
  font-variant-numeric: tabular-nums;
  background: rgba(0, 0, 0, 0.2);
  box-shadow:
    inset 2px 2px 4px var(--neu-dark),
    inset -2px -2px 4px var(--neu-light);
}

.settings__btn {
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  color: var(--text-2);
  border-radius: var(--radius-md);
  padding: 7px 16px;
  cursor: pointer;
  font-size: 12px;
  outline: none;
  transition:
    color var(--motion-fast) ease,
    box-shadow var(--motion-instant) ease,
    transform 220ms var(--spring);
}
.settings__btn:hover {
  color: var(--text-1);
  transform: translateY(-1px);
}
.settings__btn:active {
  transform: scale(0.97);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.settings__btn:focus-visible {
  box-shadow: 0 0 0 2px var(--accent-glow);
}
.settings__btn--danger {
  margin-top: 14px;
  color: var(--danger);
}

.settings__row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding: 11px 0;
  border-bottom: 1px solid var(--glass-border);
  font-size: 13px;
  color: var(--text-2);
}
.settings__row:last-child {
  border-bottom: none;
}
.settings__row--click {
  cursor: pointer;
  transition: color var(--motion-fast) ease;
}
.settings__row--click:hover {
  color: var(--text-1);
}
.settings__select {
  width: 100%;
}
.settings__select--slim {
  width: 190px;
}

/* ───────── Toggle switch ───────── */
.toggle {
  -webkit-appearance: none;
  appearance: none;
  position: relative;
  flex-shrink: 0;
  width: 44px;
  height: 24px;
  margin: 0;
  border-radius: 999px;
  border: 1px solid rgba(0, 0, 0, 0.18);
  background: var(--neu-surface-inset);
  cursor: pointer;
  outline: none;
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light);
  transition: background var(--motion-normal) ease, border-color var(--motion-normal) ease, box-shadow var(--motion-fast) ease;
}
.toggle::before {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow:
    2px 2px 5px var(--neu-dark-strong),
    -1px -1px 3px var(--neu-light);
  transition: transform 340ms var(--spring), background var(--motion-normal) ease, box-shadow var(--motion-normal) ease;
}
.toggle:active::before {
  width: 22px;
}
.toggle:checked {
  background: var(--accent-soft);
  border-color: rgba(255, 92, 71, 0.4);
}
.toggle:checked::before {
  transform: translateX(20px);
  background: var(--accent);
  box-shadow:
    0 0 10px var(--accent-glow),
    2px 2px 5px var(--neu-dark-strong);
}
.toggle:checked:active::before {
  transform: translateX(16px);
}
.toggle:focus-visible {
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light),
    0 0 0 2px var(--accent-glow);
}

/* ───────── Responsive ───────── */
@media (max-width: 560px) {
  .bg-tiles {
    grid-template-columns: repeat(3, 1fr);
  }
  .settings__bg-row {
    flex-wrap: wrap;
  }
  .settings__range {
    max-width: none;
    flex-basis: 100%;
    order: 3;
  }
}

/* ───────── Reduced motion ───────── */
@media (prefers-reduced-motion: reduce) {
  .settings *,
  .settings *::before,
  .settings *::after {
    animation: none !important;
    transition-duration: 1ms !important;
    transition-delay: 0s !important;
  }
}
</style>