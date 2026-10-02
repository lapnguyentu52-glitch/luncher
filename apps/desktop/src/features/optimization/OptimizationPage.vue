<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'

import AppSelect from '@/app/components/AppSelect.vue'
import { t } from '@/shared/i18n'
import { useInstancesStore } from '@/stores/instances.store'
import { useNotificationsStore } from '@/stores/notifications.store'
import { useOptimizationStore, type OptTab } from '@/stores/optimization.store'
import type { OptimizationChange } from '@/types/optimization'

const optimization = useOptimizationStore()
const instances = useInstancesStore()
const notifications = useNotificationsStore()

const TABS: Array<{ id: OptTab; label: () => string }> = [
  { id: 'game', label: () => t('opt.tab_game') },
  { id: 'system', label: () => t('opt.tab_system') },
]

const selectedInstanceId = ref('')
const showPlanConfirm = ref(false)

/** Options AppSelect: '' = hardware only. */
const instanceOptions = computed(() => [
  { value: '', label: t('opt.hardware_only') },
  ...instances.items.map((i) => ({ value: i.id, label: `${i.name} (${i.minecraftVersion})` })),
])

function onInstanceChange(id: string): void {
  selectedInstanceId.value = id
  void onInstanceChanged()
}

async function onInstanceChanged(): Promise<void> {
  optimization.plan = null
  await optimization.load(selectedInstanceId.value || undefined)
  await optimization.refreshSnapshot(selectedInstanceId.value)
}

onMounted(() => {
  void instances.load().then(() => {
    if (!selectedInstanceId.value && instances.items[0]) {
      selectedInstanceId.value = instances.items[0].id
    }
    void optimization.load(selectedInstanceId.value || undefined)
  })
  void optimization.loadSystem()
})

async function onPreview(profileId: string): Promise<void> {
  if (!selectedInstanceId.value) return
  const ok = await optimization.loadPlan(selectedInstanceId.value, profileId)
  if (!ok) {
    notifications.push('error', t('opt.notif_plan_failed'), `${optimization.errorCode}: ${optimization.errorMessage}`, 'optimization')
  }
}

function onApply(): void {
  if (!optimization.plan?.hasChanges) return
  showPlanConfirm.value = true
}

async function onApplyConfirm(): Promise<void> {
  showPlanConfirm.value = false
  const ok = await optimization.apply(selectedInstanceId.value)
  if (ok) {
    notifications.push('success', t('opt.notif_applied'), optimization.selectedProfileId, 'optimization')
  } else {
    notifications.push('error', t('opt.notif_apply_failed'), `${optimization.errorCode}: ${optimization.errorMessage}`, 'optimization')
  }
}

async function onRollback(): Promise<void> {
  const ok = await optimization.rollback(selectedInstanceId.value)
  if (ok) {
    notifications.push('success', t('opt.notif_rolled_back'), t('opt.notif_rollback_hint'), 'optimization')
  } else {
    notifications.push('error', t('opt.notif_rollback_failed'), `${optimization.errorCode}: ${optimization.errorMessage}`, 'optimization')
  }
}

function changeLabel(change: OptimizationChange): string {
  const before = change.before === null || change.before === undefined ? '(unset)' : String(change.before)
  const after = change.after === null || change.after === undefined ? '(unset)' : String(change.after)
  return `${change.field}: ${before} → ${after}`
}

function formatBytes(bytes: number | undefined): string {
  const v = bytes ?? 0
  if (v < 1024) return `${v} B`
  if (v < 1024 * 1024) return `${(v / 1024).toFixed(1)} KB`
  if (v < 1024 * 1024 * 1024) return `${(v / 1024 / 1024).toFixed(1)} MB`
  return `${(v / 1024 / 1024 / 1024).toFixed(2)} GB`
}
</script>

<template>
  <section class="opt">
    <header class="opt__header">
      <h1 class="opt__title">
        {{ t('opt.title') }}
      </h1>
      <div
        class="opt__tabs"
        role="tablist"
      >
        <button
          v-for="tab in TABS"
          :key="tab.id"
          class="opt__tab"
          :class="{ 'opt__tab--active': optimization.tab === tab.id }"
          type="button"
          role="tab"
          :aria-selected="optimization.tab === tab.id"
          @click="optimization.setTab(tab.id)"
        >
          {{ tab.label() }}
        </button>
      </div>
    </header>

    <!-- Loading §158 -->
    <div
      v-if="optimization.phase === 'loading'"
      class="opt__state"
    >
      <div
        v-for="n in 4"
        :key="n"
        class="skeleton"
      />
    </div>

    <!-- Offline §68 -->
    <div
      v-else-if="optimization.phase === 'offline'"
      class="opt__state opt__state--warn"
    >
      <p>{{ t('opt.offline') }}</p>
      <button
        class="opt-btn"
        type="button"
        @click="optimization.load(selectedInstanceId || undefined)"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <!-- Error §160 -->
    <div
      v-else-if="optimization.phase === 'error'"
      class="opt__state opt__state--error"
    >
      <p>{{ t('opt.error') }}</p>
      <p class="opt__hint">
        {{ optimization.errorCode }}: {{ optimization.errorMessage }}
      </p>
      <button
        class="opt-btn"
        type="button"
        @click="optimization.load(selectedInstanceId || undefined)"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <template v-else>
      <!-- ===== GAME TAB (§3/§70) ===== -->
      <template v-if="optimization.tab === 'game'">
        <div class="opt__row">
          <label class="opt__field">
            <span>{{ t('common.instance') }}</span>
            <AppSelect
              :model-value="selectedInstanceId"
              :options="instanceOptions"
              :aria-label="t('common.instance')"
              @update:model-value="onInstanceChange"
            />
          </label>
        </div>

        <!-- Scan summary -->
        <div
          v-if="optimization.scan"
          class="opt__grid"
        >
          <div class="opt-card">
            <h2 class="opt__subtitle">
              {{ t('opt.hardware') }}
            </h2>
            <div class="opt__row-line">
              <span>RAM</span><code>{{ formatBytes(optimization.scan.hardware.ramTotalMb * 1024 * 1024) }}</code>
            </div>
            <div class="opt__row-line">
              <span>{{ t('opt.cpu_threads') }}</span><code>{{ optimization.scan.hardware.cpuThreads }}</code>
            </div>
            <div class="opt__row-line">
              <span>{{ t('opt.recommended') }}</span><code>{{ optimization.scan.profile }}</code>
            </div>
          </div>
          <div class="opt-card">
            <h2 class="opt__subtitle">
              {{ t('opt.current') }}
            </h2>
            <div class="opt__row-line">
              <span>{{ t('common.instance') }}</span>
              <code>{{ optimization.scan.instance?.name ?? '—' }}</code>
            </div>
            <div class="opt__row-line">
              <span>Profile</span>
              <code>{{ optimization.scan.currentProfile ?? 'none' }}</code>
            </div>
            <div class="opt__row-line">
              <span>{{ t('opt.snapshot') }}</span>
              <code>{{ optimization.snapshot ? optimization.snapshot.file : '—' }}</code>
            </div>
          </div>
        </div>

        <!-- Warnings / bottlenecks -->
        <div
          v-if="optimization.scan && (optimization.scan.warnings.length || optimization.scan.bottlenecks.length)"
          class="opt-card"
        >
          <div
            v-for="warning in optimization.scan.warnings"
            :key="warning"
            class="opt__note opt__note--warning"
          >
            {{ warning }}
          </div>
          <div
            v-for="bottleneck in optimization.scan.bottlenecks"
            :key="bottleneck"
            class="opt__note"
          >
            {{ bottleneck }}
          </div>
        </div>

        <!-- Profiles §3.1 -->
        <div
          v-if="optimization.scan"
          class="opt-card"
        >
          <h2 class="opt__subtitle">
            {{ t('opt.profiles') }}
          </h2>
          <div class="opt__profile-grid">
            <button
              v-for="profile in optimization.scan.profiles"
              :key="profile.id"
              class="opt-btn opt-btn--chip"
              type="button"
              :class="{ 'opt-btn--chip-on': optimization.selectedProfileId === profile.id }"
              @click="onPreview(profile.id)"
            >
              {{ profile.labelKey }}
            </button>
          </div>
        </div>

        <!-- Plan preview §3.1/§70: Before → After -->
        <div
          v-if="optimization.plan"
          class="opt-card"
        >
          <h2 class="opt__subtitle">
            {{ t('opt.plan', { id: optimization.plan.profileId }) }}
          </h2>
          <div
            v-if="!optimization.plan.hasChanges"
            class="opt__hint"
          >
            {{ t('opt.no_changes') }}
          </div>
          <template v-else>
            <div
              v-for="change in optimization.plan.jvm"
              :key="`jvm-${change.field}`"
              class="opt__diff"
            >
              <code>{{ changeLabel(change) }}</code>
            </div>
            <div
              v-for="change in optimization.plan.minecraft"
              :key="`mc-${change.field}`"
              class="opt__diff"
            >
              <code>{{ changeLabel(change) }}</code>
            </div>
          </template>
          <div class="opt__actions">
            <button
              class="opt-btn opt-btn--primary"
              type="button"
              :disabled="optimization.applying || !optimization.plan.hasChanges"
              @click="onApply"
            >
              {{ optimization.applying ? t('opt.applying') : t('opt.apply') }}
            </button>
          </div>
        </div>

        <!-- Rollback §70 -->
        <div
          v-if="optimization.snapshot"
          class="opt-card"
        >
          <h2 class="opt__subtitle">
            Rollback
          </h2>
          <div class="opt__row-line">
            <span>{{ t('opt.snapshot') }}</span>
            <code>{{ optimization.snapshot.file }}</code>
          </div>
          <div
            v-if="optimization.snapshot.profile"
            class="opt__row-line"
          >
            <span>{{ t('opt.profile_applied') }}</span>
            <code>{{ optimization.snapshot.profile }}</code>
          </div>
          <button
            class="opt-btn opt-btn--danger"
            type="button"
            :disabled="optimization.busy"
            @click="onRollback"
          >
            {{ optimization.busy ? t('opt.rolling_back') : t('opt.rollback_to') }}
          </button>
        </div>
      </template>

      <!-- ===== SYSTEM TAB (§4/§35–§37) ===== -->
      <template v-else>
        <div
          v-if="optimization.overview"
          class="opt__grid"
        >
          <div class="opt-card">
            <h2 class="opt__subtitle">
              {{ t('opt.hardware') }}
            </h2>
            <div class="opt__row-line">
              <span>RAM</span>
              <code>{{ formatBytes(optimization.overview.hardware.ramTotalMb * 1024 * 1024)
              }}<template v-if="optimization.overview.hardware.ramPercent">
                · {{ optimization.overview.hardware.ramPercent }}%
              </template></code>
            </div>
            <div class="opt__row-line">
              <span>{{ t('opt.cpu_threads') }}</span>
              <code>{{ optimization.overview.hardware.cpuThreads }}</code>
            </div>
            <div
              v-if="optimization.overview.hardware.battery"
              class="opt__row-line"
            >
              <span>{{ t('opt.battery') }}</span>
              <code>{{ optimization.overview.hardware.battery.percent }}% {{ optimization.overview.hardware.battery.plugged ? t('opt.plugged') : '' }}</code>
            </div>
          </div>
          <div class="opt-card">
            <h2 class="opt__subtitle">
              {{ t('opt.power_plan') }}
            </h2>
            <div class="opt__row-line">
              <span>{{ t('opt.current') }}</span>
              <code>{{ optimization.power?.plan ?? optimization.overview.power.plan }}</code>
            </div>
            <div class="opt__profile-grid">
              <button
                v-for="plan in optimization.power?.plans ?? optimization.overview.power.plans ?? []"
                :key="plan.id"
                class="opt-btn opt-btn--chip"
                type="button"
                :class="{ 'opt-btn--chip-on': (optimization.power?.plan ?? '') === plan.id }"
                :disabled="optimization.busy"
                @click="optimization.switchPowerPlan(plan.id)"
              >
                {{ plan.labelKey ?? plan.id }}
              </button>
            </div>
          </div>
        </div>

        <!-- Cleanup §35/§37 -->
        <div class="opt-card">
          <div class="opt__cleanup-head">
            <h2 class="opt__subtitle">
              {{ t('opt.disk_cleanup') }}
            </h2>
            <button
              class="opt-btn"
              type="button"
              :disabled="optimization.busy"
              @click="optimization.rescanCleanup()"
            >
              {{ t('opt.rescan') }}
            </button>
          </div>
          <div
            v-if="optimization.cleanup"
            class="opt__hint"
          >
            {{ t('opt.total', { v: formatBytes(optimization.cleanup.totalBytes) }) }}
          </div>
          <div
            v-for="item in optimization.cleanup?.items ?? []"
            :key="item.path"
            class="opt__cleanup-item"
          >
            <label class="opt__check">
              <input
                type="checkbox"
                :checked="optimization.selectedCleanupPaths.includes(item.path)"
                @change="optimization.toggleCleanupPath(item.path)"
              >
              <span>{{ item.label ?? item.path }}</span>
            </label>
            <code>{{ formatBytes(item.bytes) }}</code>
          </div>
          <div class="opt__actions">
            <button
              class="opt-btn opt-btn--primary"
              type="button"
              :disabled="optimization.busy || optimization.selectedCleanupPaths.length === 0"
              @click="optimization.clean().then((ok) => ok
                ? notifications.push('success', t('opt.notif_cleaned'), '', 'optimization')
                : notifications.push('error', t('opt.notif_clean_failed'), `${optimization.errorCode}: ${optimization.errorMessage}`, 'optimization'))"
            >
              {{ t('opt.clean_selected') }}
            </button>
            <button
              v-if="optimization.lastCleanId"
              class="opt-btn"
              type="button"
              :disabled="optimization.busy"
              @click="optimization.undoClean().then((ok) => ok
                ? notifications.push('success', t('opt.notif_undone'), t('opt.notif_undone_hint'), 'optimization')
                : notifications.push('error', t('opt.notif_undo_failed'), `${optimization.errorCode}: ${optimization.errorMessage}`, 'optimization'))"
            >
              {{ t('opt.undo_clean') }}
            </button>
            <button
              class="opt-btn opt-btn--danger"
              type="button"
              :disabled="optimization.busy"
              @click="optimization.emptyTrash().then((removed) => removed !== null
                ? notifications.push('success', t('opt.notif_trash'), t('opt.notif_items', { n: removed }), 'optimization')
                : notifications.push('error', t('opt.notif_failed'), `${optimization.errorCode}: ${optimization.errorMessage}`, 'optimization'))"
            >
              {{ t('opt.empty_trash') }}
            </button>
          </div>
        </div>
      </template>
    </template>

    <!-- Confirm modal — apply có snapshot (§70) -->
    <div
      v-if="showPlanConfirm"
      class="opt-modal-backdrop"
      @click.self="showPlanConfirm = false"
    >
      <div
        class="opt-modal"
        role="dialog"
        aria-modal="true"
        :aria-label="t('opt.confirm_aria')"
      >
        <h2 class="opt__subtitle">
          {{ t('opt.confirm_apply') }}
        </h2>
        <p class="opt__hint">
          {{ t('opt.confirm_hint') }}
        </p>
        <div class="opt__actions">
          <button
            class="opt-btn"
            type="button"
            @click="showPlanConfirm = false"
          >
            {{ t('common.cancel') }}
          </button>
          <button
            class="opt-btn opt-btn--primary"
            type="button"
            @click="onApplyConfirm"
          >
            Apply
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.opt {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 1200px;
}
.opt__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.opt__title {
  margin: 0;
  font-size: 22px;
}
.opt__subtitle {
  margin: 0;
  font-size: 14px;
}
.opt__tabs {
  display: flex;
  gap: 4px;
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-md);
  padding: 3px;
  background: var(--neu-surface-inset);
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px var(--neu-light);
}
.opt__tab {
  background: transparent;
  border: none;
  color: var(--text-2);
  border-radius: var(--radius-sm);
  padding: 5px 14px;
  cursor: pointer;
  font-size: 12px;
}
.opt__tab--active {
  background: var(--neu-surface);
  color: var(--accent);
  box-shadow:
    2px 2px 5px var(--neu-dark),
    -2px -2px 5px var(--neu-light);
}
.opt__state {
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-lg);
  /* Frosted solid — không backdrop-filter (perf §5.3) */
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(255, 255, 255, 0) 42%),
    var(--glass-bg-solid);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark);
  padding: 24px;
  color: var(--text-2);
  font-size: 13px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-start;
}
.opt__state--warn {
  border-color: rgba(245, 189, 79, 0.4);
}
.opt__state--error {
  border-color: rgba(255, 92, 102, 0.45);
}
.opt__hint {
  color: var(--text-3);
  font-size: 12px;
}
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */
.opt__grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.opt-card {
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.opt__row {
  display: flex;
  gap: 12px;
}
.opt__row-line {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--text-3);
}
.opt__row-line code {
  color: var(--text-1);
  font-size: 11px;
  text-align: right;
  word-break: break-all;
}
.opt__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  color: var(--text-3);
  min-width: 260px;
}

.opt-btn {
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  color: var(--text-2);
  border-radius: var(--radius-md);
  padding: 7px 14px;
  cursor: pointer;
  font-size: 12px;
  transition: color var(--motion-fast) ease, box-shadow var(--motion-instant) ease;
}
.opt-btn:not(:disabled):hover {
  color: var(--text-1);
}
.opt-btn:not(:disabled):active {
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.opt-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.opt-btn--primary {
  border-color: var(--accent);
  color: var(--accent);
}
.opt-btn--danger {
  border-color: var(--danger);
  color: var(--danger);
}
.opt-btn--chip {
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
}
.opt-btn--chip-on {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-soft);
}
.opt__profile-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.opt__diff {
  border: 1px solid rgba(0, 0, 0, 0.18);
  border-radius: var(--radius-sm);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light);
  padding: 6px 9px;
}
.opt__diff code {
  font-size: 11px;
  color: var(--text-1);
  word-break: break-all;
}
.opt__note {
  font-size: 12px;
  color: var(--text-2);
  border-radius: 6px;
  padding: 5px 8px;
  background: var(--surface-2);
}
.opt__note--warning {
  background: color-mix(in srgb, var(--warning) 12%, transparent);
  color: var(--warning);
}
.opt__actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.opt__cleanup-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.opt__cleanup-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  border: 1px solid rgba(0, 0, 0, 0.18);
  border-radius: var(--radius-sm);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light);
  padding: 6px 9px;
}
.opt__cleanup-item code {
  font-size: 10px;
  color: var(--text-2);
}
.opt__check {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-2);
}
.opt-modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(4, 5, 9, 0.5);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
  display: grid;
  place-items: center;
  z-index: 60;
}
.opt-modal {
  width: min(380px, 92vw);
  background: var(--glass-bg-strong);
  backdrop-filter: blur(var(--glass-blur-strong)) saturate(160%);
  -webkit-backdrop-filter: blur(var(--glass-blur-strong)) saturate(160%);
  border: 1px solid var(--glass-border);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 24px 64px var(--neu-dark-strong);
  border-radius: var(--radius-lg);
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
