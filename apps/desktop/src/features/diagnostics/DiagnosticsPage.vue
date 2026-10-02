<script setup lang="ts">
import { useDiagnosticsStore } from '@/stores/diagnostics.store'
import { useInstancesStore } from '@/stores/instances.store'
import { useNotificationsStore } from '@/stores/notifications.store'
import { computed, onMounted, ref } from 'vue'
import AppSelect, { type AppSelectOption } from '@/app/components/AppSelect.vue'
import { t } from '@/shared/i18n'

const diagnostics = useDiagnosticsStore()
const instances = useInstancesStore()
const notifications = useNotificationsStore()

const selectedInstanceId = ref('')

/** Options AppSelect: '' = không chọn instance. */
const instanceOptions = computed<AppSelectOption[]>(() => [
  { value: '', label: t('common.no_instance') },
  ...instances.items.map((instance) => ({ value: instance.id, label: instance.name })),
])

function onInstanceChange(id: string): void {
  selectedInstanceId.value = id
  void diagnostics.load(id || undefined)
}

onMounted(() => {
  void instances.load().then(() => {
    void diagnostics.load(selectedInstanceId.value || undefined)
  })
})

function severityClass(severity: string): string {
  return severity === 'error' ? 'dg__sev-error' : 'dg__sev-warning'
}

async function onScan(action: string): Promise<void> {
  const ok = await diagnostics.loadScan(action, selectedInstanceId.value || undefined)
  if (!ok) {
    notifications.push('error', t('dg.notif_scan_failed'), `${diagnostics.errorCode}: ${diagnostics.errorMessage}`, 'diagnostics')
  }
}

async function onRun(): Promise<void> {
  const action = diagnostics.selectedAction
  if (!action) return
  const ok = await diagnostics.run(action, selectedInstanceId.value || undefined)
  if (ok) {
    notifications.push('success', t('dg.notif_repair_done'), t('dg.notif_steps', { action, n: diagnostics.runSummary?.length ?? 0 }), 'diagnostics')
  } else {
    notifications.push('error', t('dg.notif_repair_failed'), `${diagnostics.errorCode}: ${diagnostics.errorMessage}`, 'diagnostics')
  }
}

async function onExport(): Promise<void> {
  const ok = await diagnostics.export(selectedInstanceId.value || undefined)
  if (ok && diagnostics.lastExport) {
    const blob = new Blob([JSON.stringify(diagnostics.lastExport, null, 2)], {
      type: 'application/json',
    })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `antares-diagnostics-${Date.now()}.json`
    a.click()
    URL.revokeObjectURL(url)
    notifications.push('success', t('dg.notif_exported'), t('dg.notif_export_hint'), 'diagnostics')
  } else {
    notifications.push('error', t('dg.notif_export_failed'), `${diagnostics.errorCode}: ${diagnostics.errorMessage}`, 'diagnostics')
  }
}
</script>

<template>
  <section class="dg">
    <header class="dg__header">
      <h1 class="dg__title">
        {{ t('dg.title') }}
      </h1>
      <div class="dg__header-actions">
        <AppSelect
          :model-value="selectedInstanceId"
          :options="instanceOptions"
          :aria-label="t('common.instance')"
          class="dg__appselect"
          @update:model-value="onInstanceChange"
        />
        <button
          class="dg-btn dg-btn--primary"
          type="button"
          @click="onExport"
        >
          {{ diagnostics.exporting ? t('dg.exporting') : t('dg.export') }}
        </button>
      </div>
    </header>

    <!-- Loading §158 -->
    <div
      v-if="diagnostics.phase === 'loading'"
      class="dg__state"
    >
      <div
        v-for="n in 4"
        :key="n"
        class="skeleton"
      />
    </div>

    <!-- Offline §68 -->
    <div
      v-else-if="diagnostics.phase === 'offline'"
      class="dg__state dg__state--warn"
    >
      <p>{{ t('dg.offline') }}</p>
      <button
        class="dg-btn"
        type="button"
        @click="diagnostics.load(selectedInstanceId || undefined)"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <!-- Error §160 -->
    <div
      v-else-if="diagnostics.phase === 'error'"
      class="dg__state dg__state--error"
    >
      <p>{{ t('dg.error') }}</p>
      <p class="dg__hint">
        {{ diagnostics.errorCode }}: {{ diagnostics.errorMessage }}
      </p>
      <button
        class="dg-btn"
        type="button"
        @click="diagnostics.load(selectedInstanceId || undefined)"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <template v-else>
      <!-- Insights §41 -->
      <div class="dg-card">
        <div class="dg__card-head">
          <h2 class="dg__subtitle">
            {{ t('dg.insights') }}
          </h2>
          <div class="dg__hint">
            {{ t('dg.error_warn', { e: diagnostics.insights?.errorCount ?? 0, w: diagnostics.insights?.warnCount ?? 0 }) }}
            <button
              class="dg-btn dg-btn--mini"
              type="button"
              :disabled="diagnostics.analyzing"
              @click="diagnostics.reanalyze(selectedInstanceId || undefined)"
            >
              {{ diagnostics.analyzing ? '…' : t('dg.reanalyze') }}
            </button>
          </div>
        </div>
        <div
          v-if="(diagnostics.insights?.insights.length ?? 0) === 0"
          class="dg__hint"
        >
          {{ t('dg.no_issues') }}
        </div>
        <div
          v-for="insight in diagnostics.insights?.insights ?? []"
          :key="insight.id"
          class="dg__insight"
        >
          <span
            class="dg__sev"
            :class="severityClass(insight.severity)"
          >{{ insight.severity }}</span>
          <div class="dg__insight-body">
            <div class="dg__insight-head">
              <code>{{ insight.id }}</code>
              <span class="dg__hint">×{{ insight.count }} · {{ insight.sources.join(', ') }}</span>
            </div>
            <div class="dg__hint">
              {{ insight.recommend }} → {{ insight.action.target }}
            </div>
            <code
              v-for="line in insight.lines"
              :key="`${line.source}-${line.line}`"
              class="dg__line"
            >
              {{ line.source }}:{{ line.line }} — {{ line.text }}
            </code>
          </div>
        </div>
      </div>

      <!-- Log center §16/§41 -->
      <div class="dg-card">
        <div class="dg__card-head">
          <h2 class="dg__subtitle">
            {{ t('dg.log_center') }}
          </h2>
          <div class="dg__tabs-mini">
            <button
              v-for="source in diagnostics.sources"
              :key="source.id"
              class="dg-btn dg-btn--chip"
              type="button"
              :class="{ 'dg-btn--chip-on': diagnostics.selectedSource === source.id }"
              :disabled="!source.available"
              @click="diagnostics.read(source.id, selectedInstanceId || undefined)"
            >
              {{ source.id }}<template v-if="source.available">
                · {{ source.lines || Math.ceil(source.bytes / 1024) + 'KB' }}
              </template>
            </button>
          </div>
        </div>
        <div
          v-if="diagnostics.logLoading"
          class="dg__hint"
        >
          {{ t('common.loading') }}
        </div>
        <div
          v-else-if="!diagnostics.log || diagnostics.log.lines.length === 0"
          class="dg__hint"
        >
          {{ t('dg.log_empty') }}
        </div>
        <div
          v-else
          class="dg__log"
        >
          <div
            v-for="line in diagnostics.log.lines"
            :key="line.n"
            class="dg__log-row"
          >
            <span class="dg__log-n">{{ line.n }}</span>
            <code>{{ line.text }}</code>
          </div>
          <div
            v-if="diagnostics.log.truncated"
            class="dg__hint"
          >
            {{ t('dg.log_truncated', { n: diagnostics.log.lines.length }) }}
          </div>
        </div>
      </div>

      <!-- Repair §39 — scan-first -->
      <div class="dg-card">
        <h2 class="dg__subtitle">
          {{ t('dg.repair') }}
        </h2>
        <div class="dg__actions">
          <button
            v-for="action in diagnostics.actions"
            :key="action"
            class="dg-btn dg-btn--chip"
            type="button"
            :class="{ 'dg-btn--chip-on': diagnostics.selectedAction === action }"
            :disabled="diagnostics.scanning"
            @click="onScan(action)"
          >
            {{ action }}
          </button>
        </div>

        <div
          v-if="diagnostics.scan"
          class="dg__scan"
        >
          <div class="dg__hint">
            {{ diagnostics.scan.hasIssues
              ? t('dg.scan_steps', { n: diagnostics.scan.planned.length })
              : t('dg.scan_clean') }}
          </div>
          <div
            v-for="(step, i) in diagnostics.scan.planned"
            :key="i"
            class="dg__scan-step"
          >
            <code>{{ step.kind }}: {{ step.detail ?? step.path }}</code>
          </div>
          <button
            v-if="diagnostics.scan.hasIssues"
            class="dg-btn dg-btn--primary"
            type="button"
            :disabled="diagnostics.running"
            @click="onRun"
          >
            {{ diagnostics.running ? t('dg.running') : t('dg.run_repair') }}
          </button>
        </div>

        <div
          v-if="diagnostics.runSummary"
          class="dg__scan"
        >
          <div class="dg__hint">
            {{ t('dg.done') }}
          </div>
          <div
            v-for="(done, i) in diagnostics.runSummary"
            :key="i"
            class="dg__scan-step"
          >
            <code>✓ {{ done }}</code>
          </div>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.dg {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 1200px;
}
.dg__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.dg__title {
  margin: 0;
  font-size: 22px;
}
.dg__subtitle {
  margin: 0;
  font-size: 14px;
}
.dg__header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.dg__state {
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
.dg__state--warn {
  border-color: rgba(245, 189, 79, 0.4);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark),
    0 0 20px rgba(245, 189, 79, 0.1);
}
.dg__state--error {
  border-color: rgba(255, 92, 102, 0.45);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark),
    0 0 20px rgba(255, 92, 102, 0.12);
}
.dg__hint {
  color: var(--text-3);
  font-size: 12px;
}
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */
.dg-card {
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
.dg__card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}
.dg__appselect {
  width: 220px;
  flex-shrink: 0;
}
.dg-btn {
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
.dg-btn:not(:disabled):hover {
  color: var(--text-1);
}
.dg-btn:not(:disabled):active {
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.dg-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.dg-btn--primary {
  border-color: var(--accent);
  color: var(--accent);
}
.dg-btn--mini {
  padding: 2px 8px;
  font-size: 10px;
}
.dg-btn--chip {
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
}
.dg-btn--chip-on {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-soft);
}
.dg__tabs-mini {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.dg__insight {
  display: flex;
  gap: 8px;
  border: 1px solid rgba(0, 0, 0, 0.18);
  border-radius: var(--radius-md);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 6px var(--neu-dark-strong),
    inset -2px -2px 6px var(--neu-light);
  padding: 10px;
}
.dg__sev {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  align-self: flex-start;
  padding: 2px 6px;
  border-radius: 4px;
}
.dg__sev-error {
  background: color-mix(in srgb, var(--danger) 15%, transparent);
  color: var(--danger);
}
.dg__sev-warning {
  background: color-mix(in srgb, var(--warning) 15%, transparent);
  color: var(--warning);
}
.dg__insight-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  flex: 1;
}
.dg__insight-head {
  display: flex;
  gap: 8px;
  align-items: baseline;
}
.dg__insight-head code {
  font-size: 12px;
  color: var(--text-1);
}
.dg__line {
  display: block;
  font-size: 10px;
  color: var(--text-3);
  word-break: break-all;
  border-left: 2px solid var(--border-1);
  padding-left: 6px;
}
.dg__log {
  max-height: 320px;
  overflow-y: auto;
  border: 1px solid rgba(0, 0, 0, 0.2);
  border-radius: var(--radius-md);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px var(--neu-light);
  padding: 8px 10px;
  font-family: ui-monospace, monospace;
}
.dg__log-row {
  display: flex;
  gap: 8px;
  font-size: 10px;
  padding: 1px 0;
}
.dg__log-n {
  color: var(--text-3);
  min-width: 40px;
  text-align: right;
}
.dg__log-row code {
  color: var(--text-2);
  word-break: break-all;
  white-space: pre-wrap;
}
.dg__actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.dg__scan {
  border: 1px dashed var(--glass-border);
  border-radius: 8px;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.dg__scan-step code {
  font-size: 11px;
  color: var(--text-2);
  word-break: break-all;
}
</style>
