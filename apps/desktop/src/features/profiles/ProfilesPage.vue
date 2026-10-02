<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'

import { t } from '@/shared/i18n'
import { useProfilesStore } from '@/stores/profiles.store'
import { useNotificationsStore } from '@/stores/notifications.store'

const profiles = useProfilesStore()
const notifications = useNotificationsStore()

const selectedId = ref<string | null>(null)
const lastAppliedName = ref<string | null>(null)

onMounted(() => {
  void profiles.load()
})

const selected = computed(() => profiles.items.find((p) => p.id === selectedId.value) ?? null)

async function onSelect(id: string): Promise<void> {
  selectedId.value = id
  await profiles.refreshPlan(id)
}

async function onApply(): Promise<void> {
  if (!selectedId.value) return
  const ok = await profiles.apply(selectedId.value)
  if (ok) {
    lastAppliedName.value = selected.value?.name ?? null
    notifications.push(
      'success',
      t('profiles.notif_applied'),
      t('profiles.notif_applied_hint', { name: selected.value?.name ?? selectedId.value }),
      'profiles',
    )
  } else {
    notifications.push(
      'error',
      t('profiles.notif_apply_failed'),
      `${profiles.errorCode}: ${profiles.errorMessage}`,
      'profiles',
    )
  }
}

async function onRevert(): Promise<void> {
  const ok = await profiles.revert()
  if (ok) {
    lastAppliedName.value = null
    notifications.push('info', t('profiles.notif_reverted'), t('profiles.notif_reverted_hint'), 'profiles')
  } else {
    notifications.push(
      'error',
      t('profiles.notif_revert_failed'),
      `${profiles.errorCode}: ${profiles.errorMessage}`,
      'profiles',
    )
  }
}

const canApply = computed(
  () => selectedId.value !== null && profiles.plan !== null && !profiles.applying,
)

/* ───── Plan rows: gộp mọi loại thay đổi thành một danh sách để render + stagger ───── */
interface LooseChange {
  before?: unknown
  after?: unknown
  afterName?: string | null
}
interface PlanRow {
  key: string
  group: string | null
  field: string
  before: string
  after: string
}

function show(value: unknown): string {
  return value === null || value === undefined ? '—' : String(value)
}

function toRow(key: string, group: string | null, field: string, change: LooseChange): PlanRow {
  return {
    key,
    group,
    field,
    before: show(change.before),
    after: change.afterName ?? show(change.after),
  }
}

const planRows = computed<PlanRow[]>(() => {
  const changes = profiles.plan?.changes
  if (!changes) return []
  const rows: PlanRow[] = []
  if (changes.account) rows.push(toRow('account', null, t('profiles.account'), changes.account))
  if (changes.instance) rows.push(toRow('instance', null, t('common.instance'), changes.instance))
  for (const c of changes.jvm) rows.push(toRow(`jvm-${c.field}`, 'JVM', c.field, c))
  for (const c of changes.game) rows.push(toRow(`game-${c.field}`, 'Game', c.field, c))
  for (const c of changes.launch) rows.push(toRow(`launch-${c.field}`, 'Launch', c.field, c))
  return rows
})

/** Khóa để Transition biết khi nào pane bên phải cần đổi nội dung. */
const paneKey = computed(() => {
  if (profiles.planLoading) return 'loading'
  return selected.value ? `sel-${selected.value.id}` : 'none'
})
</script>

<template>
  <section class="profiles">
    <header class="profiles__header">
      <h1 class="profiles__title">
        {{ t('profiles.title') }}
      </h1>
      <button
        class="profiles__reload"
        type="button"
        :disabled="profiles.phase === 'loading'"
        @click="profiles.load()"
      >
        <svg
          class="profiles__reload-icon"
          :class="{ 'profiles__reload-icon--spin': profiles.phase === 'loading' }"
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
        v-if="profiles.phase === 'loading'"
        key="loading"
        class="profiles__state profiles__state--loading"
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
        v-else-if="profiles.phase === 'offline'"
        key="offline"
        class="profiles__state profiles__state--warn"
        role="status"
      >
        <svg
          class="profiles__state-icon"
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
        <p class="profiles__state-title">
          {{ t('profiles.offline') }}
        </p>
        <button
          class="profiles__retry"
          type="button"
          @click="profiles.load()"
        >
          {{ t('common.retry') }}
        </button>
      </div>

      <!-- Error state §160 -->
      <div
        v-else-if="profiles.phase === 'error'"
        key="error"
        class="profiles__state profiles__state--error"
        role="alert"
      >
        <svg
          class="profiles__state-icon"
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
        <p class="profiles__state-title">
          {{ t('profiles.error') }}
        </p>
        <p class="profiles__state-hint">
          {{ profiles.errorCode }}: {{ profiles.errorMessage }}
        </p>
        <button
          class="profiles__retry"
          type="button"
          @click="profiles.load()"
        >
          {{ t('common.retry') }}
        </button>
      </div>

      <!-- Empty state -->
      <div
        v-else-if="profiles.items.length === 0"
        key="empty"
        class="profiles__state"
      >
        <svg
          class="profiles__state-icon profiles__state-icon--muted"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M4 7h16M4 12h16M4 17h10" />
        </svg>
        <p class="profiles__state-title">
          {{ t('profiles.empty') }}
        </p>
        <p class="profiles__state-hint">
          {{ t('profiles.empty_hint') }}
        </p>
      </div>

      <!-- Ready: list + plan preview (§107) -->
      <div
        v-else
        key="ready"
        class="profiles__grid"
      >
        <div class="profiles__col">
          <ul class="profile-list">
            <li
              v-for="(profile, i) in profiles.items"
              :key="profile.id"
              class="profile-list__item"
              :class="{ 'profile-list__item--selected': profile.id === selectedId }"
              :style="{ '--i': i }"
            >
              <button
                type="button"
                class="profile-list__btn"
                :aria-pressed="profile.id === selectedId"
                @click="onSelect(profile.id)"
              >
                <span class="profile-list__name">
                  {{ profile.name }}
                  <span
                    v-if="profile.active"
                    class="profile-list__badge"
                  >
                    <span
                      class="profile-list__badge-dot"
                      aria-hidden="true"
                    />ACTIVE
                  </span>
                </span>
                <span class="profile-list__meta">
                  {{ profile.lastAppliedAt ? new Date(profile.lastAppliedAt * 1000).toLocaleString() : t('profiles.never_applied') }}
                </span>
                <svg
                  class="profile-list__chev"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"
                ><path d="M9 6l6 6-6 6" /></svg>
              </button>
            </li>
          </ul>
        </div>

        <div class="profiles__col">
          <Transition
            name="swap"
            mode="out-in"
          >
            <div
              :key="paneKey"
              class="pane"
            >
              <div
                v-if="profiles.planLoading"
                class="plan-hint"
                role="status"
              >
                <span
                  class="spinner"
                  aria-hidden="true"
                />
                {{ t('profiles.computing') }}
              </div>
              <template v-else-if="selected">
                <h2 class="plan-title">
                  {{ profiles.plan?.profileName ?? selected.name }}
                </h2>

                <!-- §107 — What will change? -->
                <div
                  v-if="profiles.plan && !profiles.plan.hasChanges"
                  class="plan-hint plan-hint--ok"
                >
                  <svg
                    class="plan-hint__icon"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2.2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                  ><path d="M5.5 12.5l4.2 4.2L18.5 8" /></svg>
                  {{ t('profiles.no_changes') }}
                </div>
                <div
                  v-else-if="profiles.plan"
                  class="plan"
                >
                  <div
                    v-for="(row, i) in planRows"
                    :key="row.key"
                    class="plan__row"
                    :style="{ '--i': i }"
                  >
                    <span class="plan__field">
                      <span
                        v-if="row.group"
                        class="plan__group"
                      >{{ row.group }}</span>
                      {{ row.field }}
                    </span>
                    <span class="plan__diff">
                      <span
                        class="plan__before"
                        :title="row.before"
                      >{{ row.before }}</span>
                      <svg
                        class="plan__arrow"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                      ><path d="M5 12h14M13 6l6 6-6 6" /></svg>
                      <span
                        class="plan__after"
                        :title="row.after"
                      >{{ row.after }}</span>
                    </span>
                  </div>
                </div>

                <div class="profiles__actions">
                  <button
                    class="apply-btn"
                    :class="{ 'apply-btn--busy': profiles.applying }"
                    type="button"
                    :disabled="!canApply"
                    :aria-busy="profiles.applying"
                    @click="onApply"
                  >
                    <span
                      v-if="profiles.applying"
                      class="spinner spinner--light"
                      aria-hidden="true"
                    />
                    {{ profiles.applying ? t('profiles.applying') : t('profiles.apply') }}
                  </button>
                  <button
                    class="revert-btn"
                    type="button"
                    :disabled="profiles.reverting"
                    @click="onRevert"
                  >
                    <svg
                      class="revert-btn__icon"
                      :class="{ 'revert-btn__icon--spin': profiles.reverting }"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      aria-hidden="true"
                    ><path d="M9 14L4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3" /></svg>
                    {{ profiles.reverting ? t('profiles.reverting') : t('profiles.revert') }}
                  </button>
                </div>
                <Transition name="note">
                  <p
                    v-if="lastAppliedName"
                    class="profiles__applied-note"
                  >
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2.4"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      aria-hidden="true"
                    ><path d="M5.5 12.5l4.2 4.2L18.5 8" /></svg>
                    {{ t('profiles.last_applied', { name: lastAppliedName }) }}
                  </p>
                </Transition>
              </template>
              <div
                v-else
                class="plan-hint"
              >
                {{ t('profiles.select_hint') }}
              </div>
            </div>
          </Transition>
        </div>
      </div>
    </Transition>
  </section>
</template>

<style scoped>
.profiles {
  --ease-out: cubic-bezier(0.22, 1, 0.36, 1);
  --spring: cubic-bezier(0.34, 1.56, 0.64, 1);

  display: flex;
  flex-direction: column;
  gap: 20px;
  max-width: 960px;
}
.profiles__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.profiles__title {
  margin: 0;
  font-size: 22px;
  font-weight: 800;
  letter-spacing: 0.02em;
  background: linear-gradient(180deg, var(--text-1) 30%, var(--text-2));
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}

/* Chuyển giữa các state / pane */
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
  transition: opacity 240ms ease, transform 320ms var(--ease-out);
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
.profiles__reload,
.profiles__retry,
.revert-btn {
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
.profiles__reload:hover:not(:disabled),
.profiles__retry:hover,
.revert-btn:hover:not(:disabled) {
  color: var(--text-1);
  transform: translateY(-1px);
}
.profiles__reload:active:not(:disabled),
.profiles__retry:active,
.revert-btn:active:not(:disabled) {
  transform: scale(0.97);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.profiles__reload:focus-visible,
.profiles__retry:focus-visible,
.revert-btn:focus-visible {
  box-shadow: 0 0 0 2px var(--accent-glow);
}
.profiles__reload:disabled,
.revert-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.profiles__reload-icon,
.revert-btn__icon {
  width: 14px;
  height: 14px;
  transition: transform 500ms var(--spring);
}
.profiles__reload:hover:not(:disabled) .profiles__reload-icon {
  transform: rotate(180deg);
}
.revert-btn:hover:not(:disabled) .revert-btn__icon {
  transform: translateX(-2px) rotate(-12deg);
}
.profiles__reload-icon--spin,
.revert-btn__icon--spin {
  animation: spin 900ms linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
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
  width: 15px;
  height: 15px;
  border-color: rgba(255, 255, 255, 0.3);
  border-top-color: #fff;
}

/* ───────── State cards — glass ───────── */
.profiles__state {
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-lg);
  /* Frosted solid — không backdrop-filter (perf §5.3) */
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(255, 255, 255, 0) 42%),
    var(--glass-bg-solid, var(--neu-surface));
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
.profiles__state p {
  margin: 0;
}
.profiles__state-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-1);
}
.profiles__state-hint {
  color: var(--text-3);
  font-size: 12px;
  line-height: 1.5;
}
.profiles__state .profiles__retry {
  margin-top: 8px;
}
.profiles__state-icon {
  width: 30px;
  height: 30px;
  margin-bottom: 4px;
}
.profiles__state-icon--muted {
  color: var(--text-3);
}
.profiles__state--warn {
  border-color: rgba(245, 189, 79, 0.4);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark),
    0 0 24px rgba(245, 189, 79, 0.1);
}
.profiles__state--warn .profiles__state-icon {
  color: var(--warning);
  filter: drop-shadow(0 0 8px rgba(245, 189, 79, 0.45));
}
.profiles__state--error {
  border-color: rgba(255, 92, 102, 0.45);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark),
    0 0 24px rgba(255, 92, 102, 0.14);
}
.profiles__state--error .profiles__state-icon {
  color: var(--danger);
  filter: drop-shadow(0 0 8px rgba(255, 92, 102, 0.5));
}
.profiles__state--loading {
  align-items: stretch;
  padding: 16px;
}
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */

/* ───────── Grid ───────── */
.profiles__grid {
  display: grid;
  grid-template-columns: 1fr 1.3fr;
  gap: 16px;
  align-items: start;
}
.profiles__col {
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 16px 18px;
}
.pane {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/* ───────── Danh sách profile ───────── */
.profile-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.profile-list__item {
  animation: item-in 460ms var(--ease-out) both;
  animation-delay: calc(var(--i, 0) * 50ms);
}
@keyframes item-in {
  from {
    opacity: 0;
    transform: translateX(-10px);
  }
}
.profile-list__btn {
  position: relative;
  width: 100%;
  text-align: left;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  border-radius: var(--radius-md);
  padding: 11px 36px 11px 14px;
  cursor: pointer;
  color: var(--text-1);
  display: flex;
  flex-direction: column;
  gap: 4px;
  outline: none;
  overflow: hidden;
  transition:
    transform 240ms var(--spring),
    box-shadow var(--motion-fast) ease,
    background var(--motion-fast) ease,
    border-color var(--motion-fast) ease;
}
/* Rail accent bên trái — chỉ hiện ở mục đang chọn */
.profile-list__btn::before {
  content: '';
  position: absolute;
  left: 0;
  top: 50%;
  width: 3px;
  height: 52%;
  border-radius: 0 3px 3px 0;
  background: linear-gradient(180deg, var(--accent), rgba(255, 92, 71, 0.4));
  box-shadow: 0 0 10px var(--accent-glow);
  transform: translateY(-50%) scaleY(0);
  transition: transform 360ms var(--spring);
}
.profile-list__btn:hover {
  transform: translateX(2px);
}
.profile-list__btn:active {
  transform: scale(0.985);
}
.profile-list__btn:focus-visible {
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light),
    0 0 0 2px var(--accent-glow);
}
.profile-list__item--selected .profile-list__btn {
  border-color: rgba(255, 92, 71, 0.4);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light),
    0 0 12px var(--accent-glow);
  transform: none;
}
.profile-list__item--selected .profile-list__btn::before {
  transform: translateY(-50%) scaleY(1);
}
.profile-list__name {
  font-size: 13px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.profile-list__badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.12em;
  color: var(--success);
  background: rgba(80, 216, 144, 0.1);
  border: 1px solid rgba(80, 216, 144, 0.45);
  border-radius: 999px;
  padding: 1px 8px 1px 6px;
}
.profile-list__badge-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--success);
  box-shadow: 0 0 6px var(--success);
  animation: dot-breath 2.8s ease-in-out infinite;
}
@keyframes dot-breath {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.45;
    transform: scale(0.7);
  }
}
.profile-list__meta {
  font-size: 11px;
  color: var(--text-3);
}
.profile-list__chev {
  position: absolute;
  right: 12px;
  top: 50%;
  width: 15px;
  height: 15px;
  margin-top: -7.5px;
  color: var(--text-3);
  opacity: 0;
  transform: translateX(-4px);
  transition: opacity var(--motion-fast) ease, transform 300ms var(--spring), color var(--motion-fast) ease;
}
.profile-list__btn:hover .profile-list__chev,
.profile-list__item--selected .profile-list__chev {
  opacity: 1;
  transform: translateX(0);
}
.profile-list__item--selected .profile-list__chev {
  color: var(--accent);
}

/* ───────── Plan ───────── */
.plan-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
}
.plan {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.plan__row {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12.5px;
  padding: 8px 12px;
  background: var(--neu-surface-inset);
  border: 1px solid rgba(0, 0, 0, 0.18);
  border-radius: var(--radius-sm);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light);
  animation: item-in 420ms var(--ease-out) both;
  animation-delay: calc(var(--i, 0) * 40ms);
}
.plan__field {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 140px;
  color: var(--text-2);
}
.plan__group {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--text-3);
  background: rgba(255, 255, 255, 0.05);
  border-radius: 4px;
  padding: 1px 6px;
}
.plan__diff {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
  font-family: var(--font-mono, monospace);
}
.plan__before,
.plan__after {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.plan__before {
  color: var(--text-3);
  text-decoration: line-through;
  text-decoration-color: rgba(255, 255, 255, 0.2);
}
.plan__arrow {
  width: 13px;
  height: 13px;
  flex-shrink: 0;
  color: var(--accent);
  opacity: 0.85;
}
.plan__after {
  color: var(--success);
  font-weight: 600;
}
.plan-hint {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--text-3);
  line-height: 1.5;
}
.plan-hint--ok {
  color: var(--text-2);
}
.plan-hint__icon {
  width: 18px;
  height: 18px;
  padding: 3px;
  border-radius: 50%;
  color: var(--success);
  background: rgba(80, 216, 144, 0.12);
  box-shadow: 0 0 8px rgba(80, 216, 144, 0.25);
  flex-shrink: 0;
}

/* ───────── Actions ───────── */
.profiles__actions {
  display: flex;
  gap: 10px;
  align-items: center;
}
.apply-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 12px 22px;
  font-size: 13px;
  font-weight: 800;
  letter-spacing: 0.12em;
  color: #fff;
  /* Lớp 1: vệt sáng quét ngang · Lớp 2: nền accent */
  background:
    linear-gradient(105deg, transparent 40%, rgba(255, 255, 255, 0.28) 50%, transparent 60%) -140% 0 / 250% 100% no-repeat,
    linear-gradient(180deg, var(--accent) 0%, #d4402f 100%);
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: var(--radius-md);
  cursor: pointer;
  outline: none;
  box-shadow:
    4px 4px 10px var(--neu-dark-strong),
    -3px -3px 8px var(--neu-light),
    inset 0 1px 0 rgba(255, 255, 255, 0.3),
    0 6px 20px var(--accent-glow);
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
  transition:
    transform 240ms var(--spring),
    box-shadow var(--motion-fast) ease,
    opacity var(--motion-fast) ease,
    background-position 760ms var(--ease-out);
}
.apply-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px rgba(255, 255, 255, 0.05);
}
.apply-btn:not(:disabled):hover {
  transform: translateY(-2px);
  background-position: 140% 0, 0 0;
  box-shadow:
    4px 6px 14px var(--neu-dark-strong),
    -3px -3px 8px var(--neu-light),
    inset 0 1px 0 rgba(255, 255, 255, 0.3),
    0 10px 28px var(--accent-glow);
}
.apply-btn:not(:disabled):active {
  transform: translateY(1px) scale(0.98);
  box-shadow:
    inset 4px 4px 10px rgba(0, 0, 0, 0.4),
    inset -2px -2px 6px rgba(255, 255, 255, 0.12);
}
.apply-btn:focus-visible {
  outline: 2px solid #fff;
  outline-offset: 3px;
}
/* Đang apply: giữ độ sáng nút, vệt sáng chạy liên tục */
.apply-btn--busy:disabled {
  opacity: 1;
  cursor: progress;
  box-shadow:
    4px 4px 10px var(--neu-dark-strong),
    -3px -3px 8px var(--neu-light),
    inset 0 1px 0 rgba(255, 255, 255, 0.3),
    0 6px 20px var(--accent-glow);
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

.profiles__applied-note {
  display: flex;
  align-items: center;
  gap: 8px;
  align-self: flex-start;
  margin: 0;
  padding: 4px 12px 4px 8px;
  border-radius: 999px;
  font-size: 11px;
  color: var(--info);
  background: rgba(0, 0, 0, 0.2);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark),
    inset -2px -2px 5px var(--neu-light);
}
.profiles__applied-note svg {
  width: 13px;
  height: 13px;
  flex-shrink: 0;
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
  .profiles__grid {
    grid-template-columns: 1fr;
  }
  .plan__row {
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
  }
  .plan__field {
    flex: none;
  }
}

/* ───────── Reduced motion ───────── */
@media (prefers-reduced-motion: reduce) {
  .profiles *,
  .profiles *::before,
  .profiles *::after {
    animation-duration: 1ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 1ms !important;
    transition-delay: 0s !important;
  }
}
</style>