<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'

import { useInstancesStore } from '@/stores/instances.store'
import { useNotificationsStore } from '@/stores/notifications.store'
import { launchInstance } from '@/services/flowsCommands'
import { toAntaresError } from '@/services/ipc'
import type { PreflightStatus } from '@/types/instances'
import AppSelect from '@/app/components/AppSelect.vue'
import { t } from '@/shared/i18n'

const instances = useInstancesStore()
const notifications = useNotificationsStore()

const launching = ref(false)
const launchedTaskId = ref<string | null>(null)

onMounted(() => {
  void instances.load()
})

const selectedId = computed({
  get: () => instances.selectedId,
  set: (id: string | null) => {
    if (id !== null) void instances.select(id)
  },
})

async function onSelectAndPreflight(id: string): Promise<void> {
  await instances.select(id)
  await instances.refreshPreflight()
}

async function onPlay(): Promise<void> {
  if (!instances.selectedId || !instances.preflight?.canPlay) return
  launching.value = true
  launchedTaskId.value = null
  try {
    const task = await launchInstance(instances.selectedId)
    launchedTaskId.value = task.id
    notifications.push(
      'success',
      'Launch started',
      `${instances.selected?.name ?? 'Instance'} — task ${task.id}`,
      'play',
    )
  } catch (err) {
    const antares = toAntaresError(err)
    notifications.push('error', 'Launch failed', `${antares.code}: ${antares.message}`, 'play')
  } finally {
    launching.value = false
  }
}

const canPlay = computed(
  () =>
    instances.selectedId !== null &&
    instances.preflight !== null &&
    instances.preflight.canPlay &&
    !instances.preflightLoading &&
    !launching.value,
)

/** Path SVG cho từng trạng thái preflight (thay ký tự ✓ ! ✕ — nét sắc hơn, căn giữa chuẩn). */
function statusPath(status: PreflightStatus): string {
  if (status === 'pass') return 'M5.5 12.5l4.2 4.2L18.5 8'
  if (status === 'warning') return 'M12 7v6.5M12 17h.01'
  return 'M7.5 7.5l9 9M16.5 7.5l-9 9'
}

/** Options cho AppSelect thay native select. */
const instanceOptions = computed(() =>
  instances.items.map((instance) => ({
    value: instance.id,
    label: `${instance.name} — ${instance.loader} ${instance.minecraftVersion}`,
  })),
)
</script>

<template>
  <section class="play">
    <header class="play__header">
      <h1 class="play__title">
        {{ t('play.title') }}
      </h1>
      <button
        class="play__reload"
        type="button"
        :disabled="instances.phase === 'loading'"
        @click="instances.load()"
      >
        <svg
          class="play__reload-icon"
          :class="{ 'play__reload-icon--spin': instances.phase === 'loading' }"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7" />
        </svg>
        {{ t('common.reload') }}
      </button>
    </header>

    <Transition
      name="phase"
      mode="out-in"
    >
      <!-- Loading state §158 -->
      <div
        v-if="instances.phase === 'loading'"
        key="loading"
        class="play__state play__state--loading"
        aria-busy="true"
      >
        <div
          v-for="n in 3"
          :key="n"
          class="skeleton"
        />
      </div>

      <!-- Offline state §68/§161 -->
      <div
        v-else-if="instances.phase === 'offline'"
        key="offline"
        class="play__state play__state--warn"
        role="status"
      >
        <svg
          class="play__state-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M3 3l18 18M8.5 8.8A9 9 0 0 0 3 12M21 12a9 9 0 0 0-5.1-4.4M12 17.5h.01M8.5 14.5a5 5 0 0 1 2.2-1.3M15.5 14.5a5 5 0 0 0-1.1-.8" />
        </svg>
        <p class="play__state-title">
          {{ t('play.offline') }}
        </p>
        <p class="play__state-hint">
          {{ t('play.offline_hint') }}
        </p>
        <button
          class="play__retry"
          type="button"
          @click="instances.load()"
        >
          {{ t('common.retry') }}
        </button>
      </div>

      <!-- Error state §160 -->
      <div
        v-else-if="instances.phase === 'error'"
        key="error"
        class="play__state play__state--error"
        role="alert"
      >
        <svg
          class="play__state-icon"
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
        <p class="play__state-title">
          {{ t('play.error') }}
        </p>
        <p class="play__state-hint">
          {{ instances.errorCode }}: {{ instances.errorMessage }}
        </p>
        <button
          class="play__retry"
          type="button"
          @click="instances.load()"
        >
          {{ t('common.retry') }}
        </button>
      </div>

      <!-- Empty state -->
      <div
        v-else-if="instances.items.length === 0"
        key="empty"
        class="play__state"
      >
        <svg
          class="play__state-icon play__state-icon--muted"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M3.5 7.5L12 3l8.5 4.5v9L12 21l-8.5-4.5v-9zM3.5 7.5L12 12l8.5-4.5M12 12v9" />
        </svg>
        <p class="play__state-title">
          {{ t('play.empty') }}
        </p>
        <p class="play__state-hint">
          {{ t('play.empty_hint') }}
        </p>
      </div>

      <!-- Ready -->
      <div
        v-else
        key="ready"
        class="play__grid"
      >
        <div class="play__col">
          <label class="field">
            <span class="field__label">{{ t('common.instance') }}</span>
            <AppSelect
              :model-value="selectedId ?? ''"
              :options="instanceOptions"
              aria-label="Instance"
              @update:model-value="onSelectAndPreflight"
            />
          </label>

          <!-- Preflight §8/§30 -->
          <Transition
            name="swap"
            mode="out-in"
          >
            <div
              v-if="instances.preflightLoading"
              key="pf-loading"
              class="preflight-loading"
              role="status"
            >
              <span
                class="spinner"
                aria-hidden="true"
              />
              {{ t('play.preflight_running') }}
            </div>
            <div
              v-else-if="instances.preflight"
              key="pf-ready"
              class="preflight"
            >
              <div
                v-for="(check, i) in instances.preflight.checks"
                :key="check.id"
                class="preflight__row"
                :style="{ '--i': i }"
              >
                <span
                  class="preflight__status"
                  :class="`preflight__status--${check.status}`"
                >
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2.4"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                  ><path :d="statusPath(check.status)" /></svg>
                </span>
                <span class="preflight__label">{{ check.label }}</span>
                <span class="preflight__detail">{{ check.detail }}</span>
              </div>
            </div>
            <div
              v-else
              key="pf-hint"
              class="preflight-hint"
            >
              {{ t('play.preflight_hint') }}
            </div>
          </Transition>
        </div>

        <div class="play__col play__col--launch">
          <div class="launch-summary">
            <div class="launch-summary__row">
              <span>{{ t('common.version') }}</span><b>{{ instances.selected?.minecraftVersion ?? '--' }}</b>
            </div>
            <div class="launch-summary__row">
              <span>{{ t('mods.loader') }}</span><b>{{ instances.selected?.loader ?? '--' }}</b>
            </div>
            <div class="launch-summary__row">
              <span>{{ t('play.memory') }}</span>
              <b>{{ instances.selected ? `${instances.selected.memory.maxMb} MB` : '--' }}</b>
            </div>
            <div class="launch-summary__row">
              <span>{{ t('play.launches') }}</span><b>{{ instances.selected?.launchCount ?? 0 }}</b>
            </div>
          </div>

          <button
            class="launch-btn"
            :class="{ 'launch-btn--busy': launching }"
            type="button"
            :disabled="!canPlay"
            :aria-busy="launching"
            @click="onPlay"
          >
            <span
              v-if="launching"
              class="spinner spinner--light"
              aria-hidden="true"
            />
            <svg
              v-else
              class="launch-btn__icon"
              viewBox="0 0 24 24"
              fill="currentColor"
              aria-hidden="true"
            >
              <path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z" />
            </svg>
            {{ launching ? 'LAUNCHING…' : 'PLAY' }}
          </button>

          <Transition name="note">
            <p
              v-if="launchedTaskId"
              class="launch-task"
            >
              {{ t('play.task', { id: launchedTaskId }) }}
            </p>
          </Transition>
          <p
            v-if="instances.preflight && instances.preflight.blockers > 0"
            class="launch-note launch-note--error"
          >
            {{ t('play.blockers', { n: instances.preflight.blockers }) }}
          </p>
          <p
            v-else-if="instances.preflight && instances.preflight.warnings > 0"
            class="launch-note"
          >
            {{ t('play.warnings', { n: instances.preflight.warnings }) }}
          </p>
        </div>
      </div>
    </Transition>
  </section>
</template>

<style scoped>
.play {
  --ease-out: cubic-bezier(0.22, 1, 0.36, 1);
  --spring: cubic-bezier(0.34, 1.56, 0.64, 1);

  display: flex;
  flex-direction: column;
  gap: 20px;
  max-width: 960px;
}
.play__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.play__title {
  margin: 0;
  font-size: 22px;
  font-weight: 800;
  letter-spacing: 0.02em;
  background: linear-gradient(180deg, var(--text-1) 30%, var(--text-2));
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}

/* Chuyển giữa các state */
.phase-enter-active {
  transition: opacity 340ms ease, transform 440ms var(--ease-out);
}
.phase-leave-active {
  transition: opacity 110ms ease;
}
.phase-enter-from {
  opacity: 0;
  transform: translateY(10px);
}
.phase-leave-to {
  opacity: 0;
}
.swap-enter-active {
  transition: opacity 220ms ease, transform 300ms var(--ease-out);
}
.swap-leave-active {
  transition: opacity 90ms ease;
}
.swap-enter-from {
  opacity: 0;
  transform: translateY(6px);
}
.swap-leave-to {
  opacity: 0;
}

/* ───────── Soft buttons ───────── */
.play__reload,
.play__retry {
  display: inline-flex;
  align-items: center;
  gap: 8px;
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
.play__reload:hover:not(:disabled),
.play__retry:hover {
  color: var(--text-1);
  transform: translateY(-1px);
}
.play__reload:active:not(:disabled),
.play__retry:active {
  transform: scale(0.97);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.play__reload:focus-visible,
.play__retry:focus-visible {
  box-shadow: 0 0 0 2px var(--accent-glow);
}
.play__reload:disabled {
  opacity: 0.5;
  cursor: default;
}
.play__reload-icon {
  width: 14px;
  height: 14px;
  transition: transform 500ms var(--spring);
}
.play__reload:hover:not(:disabled) .play__reload-icon {
  transform: rotate(180deg);
}
.play__reload-icon--spin {
  animation: spin 900ms linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* ───────── State cards — glass ───────── */
.play__state {
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-lg);
  /* Frosted solid — không backdrop-filter (perf §5.3) */
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(255, 255, 255, 0) 42%),
    var(--glass-bg-solid);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark);
  padding: 28px 28px 26px;
  color: var(--text-2);
  font-size: 13px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-start;
}
.play__state p {
  margin: 0;
}
.play__state-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-1);
}
.play__state-hint {
  color: var(--text-3);
  font-size: 12px;
  line-height: 1.5;
}
.play__state .play__retry {
  margin-top: 8px;
}
.play__state-icon {
  width: 30px;
  height: 30px;
  margin-bottom: 4px;
}
.play__state-icon--muted {
  color: var(--text-3);
}
.play__state--warn {
  border-color: rgba(245, 189, 79, 0.4);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark),
    0 0 24px rgba(245, 189, 79, 0.1);
}
.play__state--warn .play__state-icon {
  color: var(--warning);
  filter: drop-shadow(0 0 8px rgba(245, 189, 79, 0.45));
}
.play__state--error {
  border-color: rgba(255, 92, 102, 0.45);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark),
    0 0 24px rgba(255, 92, 102, 0.14);
}
.play__state--error .play__state-icon {
  color: var(--danger);
  filter: drop-shadow(0 0 8px rgba(255, 92, 102, 0.5));
}
.play__state--loading {
  align-items: stretch;
  padding: 16px;
}
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */

/* ───────── Grid 2 cột ───────── */
.play__grid {
  display: grid;
  grid-template-columns: 1.2fr 1fr;
  gap: 16px;
}
.play__col {
  display: flex;
  flex-direction: column;
  gap: 14px;
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 18px 20px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.field__label {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--text-3);
}

/* ───────── Preflight ───────── */
.preflight {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.preflight__row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
  color: var(--text-2);
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  animation: row-in 420ms var(--ease-out) both;
  animation-delay: calc(var(--i, 0) * 45ms);
  transition: background var(--motion-fast) ease;
}
.preflight__row:hover {
  background: rgba(255, 255, 255, 0.035);
}
@keyframes row-in {
  from {
    opacity: 0;
    transform: translateX(-8px);
  }
}
.preflight__status {
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  flex-shrink: 0;
}
.preflight__status svg {
  width: 12px;
  height: 12px;
}
.preflight__status--pass {
  color: var(--success);
  background: rgba(80, 216, 144, 0.12);
  box-shadow: 0 0 8px rgba(80, 216, 144, 0.25);
}
.preflight__status--warning {
  color: var(--warning);
  background: rgba(245, 189, 79, 0.12);
  box-shadow: 0 0 8px rgba(245, 189, 79, 0.2);
}
.preflight__status--error {
  color: var(--danger);
  background: rgba(255, 92, 102, 0.12);
  box-shadow: 0 0 8px rgba(255, 92, 102, 0.25);
}
.preflight__label {
  min-width: 90px;
  color: var(--text-1);
  font-weight: 500;
}
.preflight__detail {
  color: var(--text-3);
  font-size: 12px;
  flex: 1;
  line-height: 1.4;
}
.preflight-loading,
.preflight-hint {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--text-3);
  padding: 6px 8px;
}

.spinner {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.12);
  border-top-color: var(--accent);
  animation: spin 800ms linear infinite;
}
.spinner--light {
  width: 18px;
  height: 18px;
  border-color: rgba(255, 255, 255, 0.3);
  border-top-color: #fff;
}

/* ───────── Launch ───────── */
.launch-summary {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 13px;
  border: 1px solid var(--glass-border);
  background: rgba(0, 0, 0, 0.18);
  box-shadow:
    inset 2px 2px 6px var(--neu-dark),
    inset -2px -2px 6px var(--neu-light);
  border-radius: var(--radius-md);
  padding: 12px 14px;
}
.launch-summary__row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 3px 0;
  color: var(--text-3);
}
.launch-summary__row + .launch-summary__row {
  border-top: 1px solid rgba(255, 255, 255, 0.04);
  padding-top: 8px;
}
.launch-summary__row b {
  color: var(--text-1);
  font-weight: 600;
  text-align: right;
}

.launch-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  margin-top: 8px;
  padding: 16px;
  font-size: 18px;
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
    0 8px 26px var(--accent-glow);
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
  transition:
    transform 240ms var(--spring),
    box-shadow var(--motion-fast) ease,
    opacity var(--motion-fast) ease,
    background-position 760ms var(--ease-out);
}
.launch-btn__icon {
  width: 18px;
  height: 18px;
  filter: drop-shadow(0 1px 3px rgba(0, 0, 0, 0.35));
  transition: transform 300ms var(--spring);
}
/* Halo lan toả khi sẵn sàng */
.launch-btn::before {
  content: '';
  position: absolute;
  inset: -2px;
  border-radius: inherit;
  border: 1px solid var(--accent);
  opacity: 0;
  pointer-events: none;
}
.launch-btn:not(:disabled)::before {
  animation: halo 2.8s ease-out infinite;
}
@keyframes halo {
  0% {
    opacity: 0.55;
    transform: scale(1);
  }
  100% {
    opacity: 0;
    transform: scale(1.06, 1.35);
  }
}
.launch-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px rgba(255, 255, 255, 0.05);
}
.launch-btn:not(:disabled):hover {
  transform: translateY(-2px);
  background-position: 140% 0, 0 0;
  box-shadow:
    6px 8px 18px var(--neu-dark-strong),
    -4px -4px 12px var(--neu-light),
    inset 0 1px 0 rgba(255, 255, 255, 0.3),
    0 12px 36px var(--accent-glow);
}
.launch-btn:not(:disabled):hover .launch-btn__icon {
  transform: translateX(3px) scale(1.1);
}
.launch-btn:not(:disabled):active {
  transform: translateY(1px) scale(0.98);
  box-shadow:
    inset 4px 4px 10px rgba(0, 0, 0, 0.4),
    inset -2px -2px 6px rgba(255, 255, 255, 0.12);
}
.launch-btn:focus-visible {
  outline: 2px solid #fff;
  outline-offset: 3px;
}
/* Đang launch: vệt sáng chạy liên tục, giữ độ sáng nút (không bị mờ như disabled) */
.launch-btn--busy:disabled {
  opacity: 1;
  cursor: progress;
  box-shadow:
    6px 6px 14px var(--neu-dark-strong),
    -4px -4px 12px var(--neu-light),
    inset 0 1px 0 rgba(255, 255, 255, 0.3),
    0 8px 26px var(--accent-glow);
  animation: busy-sheen 1.4s linear infinite;
}
@keyframes busy-sheen {
  from {
    background-position: -140% 0, 0 0;
  }
  to {
    background-position: 140% 0, 0 0;
  }
}

.launch-task,
.launch-note {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
}
.launch-task {
  align-self: flex-start;
  max-width: 100%;
  padding: 4px 12px;
  border-radius: 999px;
  color: var(--info);
  background: rgba(0, 0, 0, 0.2);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark),
    inset -2px -2px 5px var(--neu-light);
  overflow-wrap: anywhere;
}
.launch-note {
  color: var(--warning);
}
.launch-note--error {
  color: var(--danger);
}
.note-enter-active {
  transition: opacity 260ms ease, transform 360ms var(--spring);
}
.note-enter-from {
  opacity: 0;
  transform: translateY(-6px) scale(0.96);
}

/* ───────── Responsive ───────── */
@media (max-width: 820px) {
  .play__grid {
    grid-template-columns: 1fr;
  }
}

/* ───────── Reduced motion ───────── */
@media (prefers-reduced-motion: reduce) {
  .play *,
  .play *::before,
  .play *::after {
    animation-duration: 1ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 1ms !important;
    transition-delay: 0s !important;
  }
}
</style>