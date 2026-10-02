<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'

import { useRouter } from 'vue-router'

import { useSessionStore } from '@/stores/session.store'
import { t } from '@/shared/i18n'
import { useInstancesStore } from '@/stores/instances.store'
import { getDashboardSummary } from '@/services/flowsCommands'
import { toAntaresError } from '@/services/ipc'
import type { DashboardSummary } from '@/types/instances'

const router = useRouter()
const session = useSessionStore()
const instances = useInstancesStore()

const summary = ref<DashboardSummary | null>(null)
const phase = ref<'loading' | 'ready' | 'error'>('loading')
const errorMessage = ref<string | null>(null)

onMounted(async () => {
  phase.value = 'loading'
  try {
    summary.value = await getDashboardSummary()
    phase.value = 'ready'
    if (summary.value?.selectedInstanceId) {
      session.selectInstance(summary.value.selectedInstanceId)
    }
    // song song load instance list để hero play enable khi đã chọn
    void instances.load()
  } catch (err) {
    const antares = toAntaresError(err)
    errorMessage.value = `${antares.code}: ${antares.message}`
    phase.value = 'error'
  }
})

const instanceLabel = computed(() => {
  if (summary.value?.recentInstanceName) return summary.value.recentInstanceName
  if (summary.value?.instanceCount) return t('dashboard.instances_count', { n: summary.value.instanceCount })
  return '--'
})

const heroSubtitle = computed(() => {
  if (!summary.value) return t('dashboard.hero_no_bridge')
  if (summary.value.account) {
    return t('dashboard.hero_greeting', { name: summary.value.account.displayName })
  }
  return t('dashboard.hero_pick_account')
})

/* Spotlight bám theo con trỏ — chỉ đổi CSS var, phần tử glow di chuyển bằng transform (GPU) */
function onHeroMove(e: PointerEvent): void {
  const el = e.currentTarget as HTMLElement
  const r = el.getBoundingClientRect()
  el.style.setProperty('--sx', `${e.clientX - r.left - 190}px`)
  el.style.setProperty('--sy', `${e.clientY - r.top - 190}px`)
}
</script>

<template>
  <section class="dashboard">
    <Transition
      name="phase"
      mode="out-in"
    >
      <!-- Loading skeleton §159: dựng sát hình dạng hero thật để không giật layout -->
      <div
        v-if="phase === 'loading'"
        key="loading"
        class="hero hero--loading"
        aria-busy="true"
      >
        <div class="hero__skeleton">
          <span class="sk sk--title" />
          <span class="sk sk--sub" />
          <div class="sk__row">
            <span class="sk sk--chip" />
            <span class="sk sk--chip" />
            <span class="sk sk--chip" />
          </div>
        </div>
        <span class="sk sk--play" />
      </div>

      <!-- Error §160 -->
      <div
        v-else-if="phase === 'error'"
        key="error"
        class="hero hero--error"
        role="alert"
      >
        <div class="hero__error-head">
          <svg
            class="hero__error-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M12 3l9.5 17h-19L12 3zM12 10v5M12 18h.01" />
          </svg>
          <h1 class="hero__title">
            {{ t('dashboard.title') }}
          </h1>
        </div>
        <p class="hero__subtitle">
          {{ t('dashboard.load_error', { msg: errorMessage ?? '' }) }}
        </p>
        <p class="hero__hint">
          {{ t('dashboard.error_hint') }}
        </p>
        <div class="hero__actions">
          <button
            class="hero__secondary"
            type="button"
            @click="router.go(0)"
          >
            {{ t('common.retry') }}
          </button>
          <RouterLink
            class="hero__secondary"
            to="/play"
          >
            {{ t('dashboard.open_play') }}
          </RouterLink>
        </div>
      </div>

      <!-- Ready -->
      <div
        v-else
        key="ready"
        class="hero"
        @pointermove="onHeroMove"
      >
        <span
          class="hero__spot"
          aria-hidden="true"
        />
        <svg
          class="hero__orbit"
          viewBox="0 0 200 200"
          fill="none"
          aria-hidden="true"
        >
          <circle
            cx="100"
            cy="100"
            r="92"
          />
          <circle
            class="hero__orbit-dash"
            cx="100"
            cy="100"
            r="68"
          />
          <circle
            cx="100"
            cy="100"
            r="44"
          />
        </svg>

        <div class="hero__info">
          <h1 class="hero__title">
            Minecraft
          </h1>
          <p class="hero__subtitle">
            {{ heroSubtitle }}
          </p>
          <div class="hero__metrics">
            <span class="hero__metric"><b>{{ t('dashboard.metric_instances') }}</b><i>{{ summary?.instanceCount ?? '--' }}</i></span>
            <span class="hero__metric"><b>{{ t('dashboard.metric_recent') }}</b><i>{{ instanceLabel }}</i></span>
            <span class="hero__metric">
              <b>{{ t('dashboard.metric_account') }}</b><i>{{ summary?.account?.displayName ?? '--' }}</i>
            </span>
          </div>
        </div>
        <button
          class="hero__play"
          type="button"
          :disabled="!summary?.instanceCount"
          :title="t('dashboard.play_title')"
          @click="router.push('/play')"
        >
          <svg
            class="hero__play-icon"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
          >
            <path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z" />
          </svg>
          PLAY
        </button>
      </div>
    </Transition>

    <div class="panels">
      <div
        class="panel"
        style="--i: 0"
      >
        <h2 class="panel__title">
          {{ t('dashboard.quick_actions') }}
        </h2>
        <div class="panel__actions">
          <RouterLink
            class="panel__action"
            to="/play"
          >
            <span>{{ t('dashboard.play_workspace') }}</span>
            <svg
              class="panel__chev"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            ><path d="M9 6l6 6-6 6" /></svg>
          </RouterLink>
          <RouterLink
            class="panel__action"
            to="/settings"
          >
            <span>{{ t('dashboard.settings') }}</span>
            <svg
              class="panel__chev"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            ><path d="M9 6l6 6-6 6" /></svg>
          </RouterLink>
        </div>
      </div>
      <div
        class="panel"
        style="--i: 1"
      >
        <h2 class="panel__title">
          {{ t('dashboard.legacy_bridge') }}
        </h2>
        <p class="panel__status">
          <span
            class="panel__dot"
            :class="{ 'panel__dot--ok': summary?.legacyAvailable }"
            aria-hidden="true"
          />
          {{ summary?.legacyAvailable ? t('dashboard.legacy_ok') : t('dashboard.legacy_off') }}
        </p>
      </div>
      <div
        class="panel"
        style="--i: 2"
      >
        <h2 class="panel__title">
          {{ t('dashboard.notifications') }}
        </h2>
        <p class="panel__empty">
          {{ t('dashboard.notifications_empty') }}
        </p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.dashboard {
  --ease-out: cubic-bezier(0.22, 1, 0.36, 1);
  --spring: cubic-bezier(0.34, 1.56, 0.64, 1);
  --ok: #3ddc97;

  display: flex;
  flex-direction: column;
  gap: 20px;
}

/* Chuyển giữa loading / error / ready */
.phase-enter-active {
  transition: opacity 360ms ease, transform 460ms var(--ease-out);
}
.phase-leave-active {
  transition: opacity 120ms ease;
}
.phase-enter-from {
  opacity: 0;
  transform: translateY(10px) scale(0.99);
}
.phase-leave-to {
  opacity: 0;
}

/* ───────── Hero ───────── */
.hero {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  min-height: 168px;
  padding: 32px 36px;
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-xl);
  /* Frosted solid — không backdrop-filter (perf §5.3) */
  background:
    radial-gradient(ellipse at 85% 20%, var(--accent-soft), transparent 55%),
    var(--glass-bg-solid);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 18px 48px var(--neu-dark);
  overflow: hidden;
  isolation: isolate;
}
.hero > * {
  position: relative;
  z-index: 1;
}

/* Spotlight theo chuột */
.hero__spot {
  position: absolute !important;
  top: 0;
  left: 0;
  z-index: 0 !important;
  width: 380px;
  height: 380px;
  border-radius: 50%;
  background: radial-gradient(circle, rgba(255, 255, 255, 0.07), transparent 65%);
  transform: translate3d(var(--sx, 60%), var(--sy, -50%), 0);
  opacity: 0;
  pointer-events: none;
  transition:
    transform 320ms var(--ease-out),
    opacity 400ms ease;
  will-change: transform;
}
.hero:hover .hero__spot {
  opacity: 1;
}

/* Quỹ đạo mờ phía sau nút PLAY — nhắc nhẹ chủ đề sao Antares */
.hero__orbit {
  position: absolute !important;
  z-index: 0 !important;
  right: -40px;
  top: 50%;
  width: 300px;
  height: 300px;
  margin-top: -150px;
  pointer-events: none;
  opacity: 0.5;
}
.hero__orbit circle {
  stroke: var(--text-3);
  stroke-width: 1;
  opacity: 0.22;
}
.hero__orbit-dash {
  stroke-dasharray: 2 8;
  stroke-linecap: round;
  transform-box: fill-box;
  transform-origin: center;
  animation: orbit-spin 60s linear infinite;
}
@keyframes orbit-spin {
  to {
    transform: rotate(360deg);
  }
}

.hero--error {
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  gap: 10px;
  border-color: rgba(255, 92, 102, 0.45);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 18px 48px var(--neu-dark),
    0 0 28px rgba(255, 92, 102, 0.14);
}
.hero__error-head {
  display: flex;
  align-items: center;
  gap: 12px;
}
.hero__error-icon {
  width: 26px;
  height: 26px;
  color: #ff5c66;
  filter: drop-shadow(0 0 8px rgba(255, 92, 102, 0.5));
}

/* ───────── Skeleton ───────── */
.hero--loading {
  pointer-events: none;
}
.hero__skeleton {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1;
}
.sk {
  display: block;
  position: relative;
  overflow: hidden;
  border-radius: var(--radius-sm);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 6px var(--neu-dark-strong),
    inset -2px -2px 6px var(--neu-light);
}
.sk::before {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(100deg, transparent 32%, rgba(255, 255, 255, 0.07) 50%, transparent 68%);
  transform: translateX(-100%);
  animation: hero-sweep 1.6s cubic-bezier(0.4, 0, 0.2, 1) infinite;
}
.sk--title {
  width: 200px;
  height: 30px;
}
.sk--sub {
  width: 300px;
  max-width: 70%;
  height: 14px;
}
.sk__row {
  display: flex;
  gap: 10px;
  margin-top: 6px;
}
.sk--chip {
  width: 110px;
  height: 26px;
}
.sk--play {
  width: 190px;
  height: 66px;
  border-radius: var(--radius-lg);
  flex-shrink: 0;
}
@keyframes hero-sweep {
  to {
    transform: translateX(100%);
  }
}

/* ───────── Hero content ───────── */
.hero__title {
  margin: 0;
  font-size: 32px;
  font-weight: 800;
  letter-spacing: 0.06em;
  background: linear-gradient(180deg, var(--text-1) 30%, var(--text-2));
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
  filter: drop-shadow(0 2px 10px rgba(0, 0, 0, 0.4));
}
.hero__subtitle {
  margin: 6px 0 18px;
  color: var(--text-2);
  font-size: 13px;
  line-height: 1.5;
}
.hero--error .hero__subtitle {
  margin: 4px 0 0;
}
.hero__hint {
  margin: 0 0 6px;
  color: var(--text-3);
  font-size: 12px;
}
.hero__actions {
  display: flex;
  gap: 10px;
}
.hero__secondary {
  padding: 8px 18px;
  border-radius: 999px;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  color: var(--text-2);
  text-decoration: none;
  font-size: 12px;
  cursor: pointer;
  outline: none;
  transition:
    color var(--motion-fast) ease,
    box-shadow var(--motion-instant) ease,
    transform 220ms var(--spring);
}
.hero__secondary:hover {
  color: var(--text-1);
  transform: translateY(-1px);
}
.hero__secondary:active {
  transform: scale(0.97);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.hero__secondary:focus-visible {
  box-shadow: 0 0 0 2px var(--accent-glow);
}

.hero__metrics {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}
.hero__metric {
  display: inline-flex;
  align-items: baseline;
  gap: 8px;
  border: 1px solid var(--glass-border);
  background: rgba(0, 0, 0, 0.22);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark),
    inset -2px -2px 5px var(--neu-light);
  border-radius: var(--radius-sm);
  padding: 5px 12px;
  font-size: 12px;
  transition: border-color var(--motion-fast) ease, transform 220ms var(--spring);
}
.hero__metric:hover {
  border-color: rgba(255, 92, 71, 0.3);
  transform: translateY(-1px);
}
.hero__metric b {
  color: var(--text-3);
  font-weight: 500;
  font-size: 11px;
  letter-spacing: 0.02em;
}
.hero__metric i {
  color: var(--text-1);
  font-style: normal;
  font-weight: 600;
}

/* ───────── PLAY — nút accent duy nhất, halo thở khi sẵn sàng ───────── */
.hero__play {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  min-width: 190px;
  padding: 18px 38px;
  font-size: 20px;
  font-weight: 800;
  letter-spacing: 0.22em;
  color: #fff;
  /* Lớp 1: vệt sáng quét ngang · Lớp 2: nền accent */
  background:
    linear-gradient(105deg, transparent 40%, rgba(255, 255, 255, 0.28) 50%, transparent 60%) -140% 0 / 250% 100% no-repeat,
    linear-gradient(180deg, var(--accent) 0%, #d4402f 100%);
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: var(--radius-lg);
  cursor: pointer;
  outline: none;
  box-shadow:
    6px 6px 14px var(--neu-dark-strong),
    -4px -4px 12px var(--neu-light),
    inset 0 1px 0 rgba(255, 255, 255, 0.3),
    0 8px 28px var(--accent-glow);
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
  transition:
    transform 240ms var(--spring),
    box-shadow var(--motion-fast) ease,
    background-position 760ms var(--ease-out);
}
.hero__play-icon {
  width: 20px;
  height: 20px;
  filter: drop-shadow(0 1px 3px rgba(0, 0, 0, 0.35));
  transition: transform 300ms var(--spring);
}
/* Halo lan toả */
.hero__play::before {
  content: '';
  position: absolute;
  inset: -2px;
  border-radius: inherit;
  border: 1px solid var(--accent);
  opacity: 0;
  pointer-events: none;
}
.hero__play:not(:disabled)::before {
  animation: play-halo 2.8s ease-out infinite;
}
@keyframes play-halo {
  0% {
    opacity: 0.55;
    transform: scale(1);
  }
  100% {
    opacity: 0;
    transform: scale(1.14, 1.3);
  }
}
.hero__play:disabled {
  cursor: not-allowed;
  opacity: 0.5;
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px rgba(255, 255, 255, 0.05);
}
.hero__play:not(:disabled):hover {
  transform: translateY(-2px);
  background-position: 140% 0, 0 0;
  box-shadow:
    6px 8px 18px var(--neu-dark-strong),
    -4px -4px 12px var(--neu-light),
    inset 0 1px 0 rgba(255, 255, 255, 0.3),
    0 12px 38px var(--accent-glow);
}
.hero__play:not(:disabled):hover .hero__play-icon {
  transform: translateX(3px) scale(1.1);
}
.hero__play:not(:disabled):active {
  transform: translateY(1px) scale(0.98);
  box-shadow:
    inset 4px 4px 10px rgba(0, 0, 0, 0.4),
    inset -2px -2px 6px rgba(255, 255, 255, 0.12);
}
.hero__play:focus-visible {
  outline: 2px solid #fff;
  outline-offset: 3px;
}

/* ───────── Panels ───────── */
.panels {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 16px;
}
.panel {
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 18px 20px;
  animation: panel-in 560ms var(--ease-out) both;
  animation-delay: calc(var(--i, 0) * 80ms + 120ms);
  transition: transform 300ms var(--ease-out), box-shadow 300ms var(--ease-out);
}
.panel:hover {
  transform: translateY(-2px);
  box-shadow:
    7px 9px 18px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
}
@keyframes panel-in {
  from {
    opacity: 0;
    transform: translateY(14px);
  }
}
.panel__title {
  margin: 0 0 14px;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--text-3);
}
.panel__actions {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.panel__action {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px 10px 14px;
  border-radius: var(--radius-md);
  background: var(--neu-surface-inset);
  border: 1px solid rgba(0, 0, 0, 0.18);
  box-shadow:
    inset 2px 2px 6px var(--neu-dark-strong),
    inset -2px -2px 6px var(--neu-light);
  color: var(--text-2);
  text-decoration: none;
  font-size: 13px;
  outline: none;
  transition:
    color var(--motion-fast) ease,
    box-shadow var(--motion-fast) ease,
    transform 160ms var(--ease-out);
}
.panel__action:hover {
  color: var(--text-1);
  box-shadow:
    inset 2px 2px 6px var(--neu-dark-strong),
    inset -2px -2px 6px var(--neu-light),
    0 0 0 1px var(--accent-glow);
}
.panel__action:active {
  transform: scale(0.98);
}
.panel__action:focus-visible {
  box-shadow:
    inset 2px 2px 6px var(--neu-dark-strong),
    inset -2px -2px 6px var(--neu-light),
    0 0 0 2px var(--accent-glow);
}
.panel__chev {
  width: 15px;
  height: 15px;
  color: var(--text-3);
  transition: transform 300ms var(--spring), color var(--motion-fast) ease;
}
.panel__action:hover .panel__chev {
  color: var(--accent);
  transform: translateX(3px);
}

.panel__status {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  color: var(--text-2);
  font-size: 13px;
}
.panel__dot {
  width: 8px;
  height: 8px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--text-3);
  box-shadow: inset 1px 1px 2px rgba(0, 0, 0, 0.5);
  transition: background var(--motion-normal) ease, box-shadow var(--motion-normal) ease;
}
.panel__dot--ok {
  background: var(--ok);
  box-shadow: 0 0 10px rgba(61, 220, 151, 0.6);
}
.panel__empty {
  margin: 0;
  color: var(--text-3);
  font-size: 13px;
}

/* ───────── Responsive ───────── */
@media (max-width: 720px) {
  .hero {
    flex-direction: column;
    align-items: stretch;
    padding: 24px 22px;
  }
  .hero__play {
    width: 100%;
  }
  .sk--play {
    width: 100%;
  }
  .hero__orbit {
    display: none;
  }
}

/* ───────── Reduced motion ───────── */
@media (prefers-reduced-motion: reduce) {
  .dashboard *,
  .dashboard *::before,
  .dashboard *::after {
    animation: none !important;
    transition-duration: 1ms !important;
    transition-delay: 0s !important;
  }
}
</style>