<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'

import AppSelect from '@/app/components/AppSelect.vue'
import { t } from '@/shared/i18n'
import { useInstancesStore } from '@/stores/instances.store'
import { useModsStore, type ModsTab } from '@/stores/mods.store'
import { useNotificationsStore } from '@/stores/notifications.store'

const instances = useInstancesStore()
const mods = useModsStore()
const notifications = useNotificationsStore()

const searchQuery = ref('')
const mrpackPath = ref('')

const modpackDone = computed(() => {
  const state = mods.modpackTask?.state
  return state === 'completed' || state === 'failed' || state === 'cancelled'
})

async function onInstallMrpack(): Promise<void> {
  if (!selectedInstance.value || !mrpackPath.value.trim()) return
  const ok = await mods.installModpack(mrpackPath.value.trim(), selectedInstance.value.id)
  if (ok) {
    notifications.push('info', t('mods.notif_modpack_started'), mrpackPath.value, 'mods')
  } else {
    notifications.push('error', t('mods.notif_modpack_failed'), `${mods.errorCode}: ${mods.errorMessage}`, 'mods')
  }
}

function onModpackDismiss(): void {
  const task = mods.modpackTask
  mods.clearModpackTask()
  if (task?.state === 'completed' && selectedInstance.value) {
    void mods.load(selectedInstance.value.id)
  }
}

const TABS: Array<{ id: ModsTab; label: () => string }> = [
  { id: 'installed', label: () => t('mods.tab_installed') },
  { id: 'discover', label: () => t('mods.tab_discover') },
  { id: 'security', label: () => t('mods.tab_security') },
]

const selectedInstance = computed(() => instances.selected)

/** Options AppSelect — instance cùng loader + MC version. */
const instanceOptions = computed(() =>
  instances.items.map((i) => ({
    value: i.id,
    label: `${i.name} — ${i.loader} ${i.minecraftVersion}`,
  })),
)
const instanceCtx = computed(
  () =>
    `${selectedInstance.value?.loader ?? 'fabric'} ${selectedInstance.value?.minecraftVersion ?? ''}`.trim(),
)

onMounted(async () => {
  await instances.load()
  const id = instances.selectedId ?? instances.items[0]?.id
  if (id) {
    await instances.select(id)
    await mods.load(id)
  }
})

async function onInstanceChange(id: string): Promise<void> {
  await instances.select(id)
  mods.scan = null
  mods.health = null
  await mods.load(id)
}

async function onRemove(filename: string): Promise<void> {
  if (!selectedInstance.value) return
  const ok = await mods.remove(selectedInstance.value.id, filename)
  if (ok) {
    if (mods.selectedDetail?.filename === filename) mods.selectDetail('', null)
    notifications.push('success', t('mods.notif_removed'), filename, 'mods')
  } else {
    notifications.push('error', t('mods.notif_remove_failed'), `${mods.errorCode}: ${mods.errorMessage}`, 'mods')
  }
}

async function onSelectMod(filename: string): Promise<void> {
  if (!selectedInstance.value) return
  await mods.selectDetail(selectedInstance.value.id, filename)
}

async function onSearch(): Promise<void> {
  if (!searchQuery.value.trim()) return
  await mods.search(searchQuery.value.trim(), selectedInstance.value?.loader ?? 'fabric',
    selectedInstance.value?.minecraftVersion ?? '1.21')
}

async function onInstall(projectId: string, title: string): Promise<void> {
  if (!selectedInstance.value) return
  const ok = await mods.install(
    selectedInstance.value.id,
    projectId,
    selectedInstance.value.loader,
    selectedInstance.value.minecraftVersion,
  )
  if (ok) {
    notifications.push('success', t('mods.notif_installed'), title, 'mods')
    mods.setTab('installed')
  } else {
    notifications.push('error', t('mods.notif_install_failed'), `${mods.errorCode}: ${mods.errorMessage}`, 'mods')
  }
}

async function onScan(): Promise<void> {
  if (!selectedInstance.value) return
  const ok = await mods.runScan(selectedInstance.value.id)
  if (ok && mods.scan) {
    notifications.push(
      mods.scan.dangerous > 0 ? 'warning' : 'success',
      t('mods.notif_scan_done'),
      t('mods.notif_scan_summary', { safe: mods.scan.safe, susp: mods.scan.suspicious, dang: mods.scan.dangerous }),
      'mods',
    )
  } else if (!ok) {
    notifications.push('error', t('mods.notif_scan_failed'), `${mods.errorCode}: ${mods.errorMessage}`, 'mods')
  }
}

async function onAutofix(): Promise<void> {
  if (!selectedInstance.value) return
  const ok = await mods.runAutofix(selectedInstance.value.id)
  if (ok) {
    notifications.push('success', t('mods.notif_autofix_done'), t('mods.notif_autofix_hint'), 'mods')
  } else {
    notifications.push('error', t('mods.notif_autofix_failed'), `${mods.errorCode}: ${mods.errorMessage}`, 'mods')
  }
}

function verdictClass(verdict: string): string {
  if (verdict === 'DANGEROUS') return 'verdict--danger'
  if (verdict === 'SUSPICIOUS') return 'verdict--warn'
  return 'verdict--ok'
}
</script>

<template>
  <section class="mods">
    <header class="mods__header">
      <h1 class="mods__title">
        {{ t('mods.title') }}
      </h1>
      <label class="mods__instance">
        <span>{{ t('common.instance') }}</span>
        <AppSelect
          :model-value="instances.selectedId ?? ''"
          :options="instanceOptions"
          :aria-label="t('common.instance')"
          class="mods__appselect"
          @update:model-value="onInstanceChange"
        />
      </label>
    </header>

    <!-- Loading state §158 -->
    <div
      v-if="mods.phase === 'loading'"
      class="mods__state"
    >
      <div
        v-for="n in 3"
        :key="n"
        class="skeleton"
      />
    </div>

    <!-- Offline §68 -->
    <div
      v-else-if="mods.phase === 'offline'"
      class="mods__state mods__state--warn"
    >
      <p>{{ t('mods.offline') }}</p>
      <button
        class="btn"
        type="button"
        @click="instances.selectedId && mods.load(instances.selectedId)"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <!-- Error §160 -->
    <div
      v-else-if="mods.phase === 'error'"
      class="mods__state mods__state--error"
    >
      <p>{{ t('mods.error') }}</p>
      <p class="mods__hint">
        {{ mods.errorCode }}: {{ mods.errorMessage }}
      </p>
      <button
        class="btn"
        type="button"
        @click="instances.selectedId && mods.load(instances.selectedId)"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <template v-else>
      <!-- Modpack install task bar (B7b — §90 task progress) -->
      <div
        v-if="mods.modpackTask"
        class="modpack-bar"
        :class="{ 'modpack-bar--done': modpackDone, 'modpack-bar--failed': mods.modpackTask.state === 'failed' }"
      >
        <div class="modpack-bar__head">
          <b>{{ t('mods.modpack_install') }}</b>
          <span class="modpack-bar__state">{{ mods.modpackTask.state }} · {{ Math.round(mods.modpackTask.progress) }}%</span>
          <button
            v-if="!modpackDone"
            class="btn"
            type="button"
            @click="mods.cancelModpackTask()"
          >
            {{ t('common.cancel') }}
          </button>
          <button
            v-else
            class="btn"
            type="button"
            @click="onModpackDismiss"
          >
            {{ t('mods.dismiss') }}
          </button>
        </div>
        <div class="modpack-bar__track">
          <div
            class="modpack-bar__fill"
            :style="`width:${Math.max(4, Math.min(100, mods.modpackTask.progress))}%`"
          />
        </div>
        <p class="mods__hint">
          {{ mods.modpackTask.message }}
          <template v-if="mods.modpackTask.error">
            — {{ mods.modpackTask.error.code }}: {{ mods.modpackTask.error.message }}
          </template>
        </p>
      </div>

      <!-- Mrpack install form (B7b) -->
      <form
        class="mrpack-form"
        @submit.prevent="onInstallMrpack"
      >
        <input
          v-model="mrpackPath"
          class="mrpack-form__input"
          type="text"
          :placeholder="t('mods.mrpack_placeholder')"
        >
        <button
          class="btn btn--primary"
          type="submit"
          :disabled="!selectedInstance || mods.modpackTask !== null && !modpackDone"
        >
          {{ t('mods.install') }} modpack
        </button>
      </form>

      <!-- Tabs §11 -->
      <nav class="mods__tabs">
        <button
          v-for="tab in TABS"
          :key="tab.id"
          type="button"
          class="mods__tab"
          :class="{ 'mods__tab--active': mods.tab === tab.id }"
          @click="mods.setTab(tab.id)"
        >
          {{ tab.label() }}
          <span
            v-if="tab.id === 'security' && (mods.issueCount > 0 || mods.dangerousCount > 0)"
            class="mods__tab-badge"
          >
            {{ mods.issueCount + mods.dangerousCount }}
          </span>
        </button>
      </nav>

      <!-- Installed -->
      <div
        v-if="mods.tab === 'installed'"
        class="mods__panel"
      >
        <div
          v-if="mods.items.length === 0"
          class="mods__state"
        >
          <p>{{ t('mods.none') }}</p>
          <p class="mods__hint">
            {{ t('mods.none_hint') }}
          </p>
        </div>
        <div class="installed-layout">
          <ul
            v-if="mods.items.length > 0"
            class="mod-list mod-list--selectable"
          >
            <li
              v-for="mod in mods.items"
              :key="mod.filename"
              class="mod-list__item"
              :class="{ 'mod-list__item--selected': mods.selectedDetail?.filename === mod.filename }"
            >
              <button
                type="button"
                class="mod-list__select"
                @click="onSelectMod(mod.filename)"
              >
                <div class="mod-list__info">
                  <span class="mod-list__name">{{ mod.name ?? mod.filename }}</span>
                  <span class="mod-list__meta">
                    {{ mod.version ?? '?' }} · {{ mod.loader ?? '?' }}
                    <template v-if="mod.readable === false">
                      · <b class="mods__warn-text">{{ t('mods.unreadable') }}</b>
                    </template>
                  </span>
                </div>
              </button>
              <button
                class="btn btn--danger"
                type="button"
                :disabled="mods.busyFilename === mod.filename"
                @click="onRemove(mod.filename)"
              >
                {{ mods.busyFilename === mod.filename ? '…' : t('mods.remove') }}
              </button>
            </li>
          </ul>

          <!-- Mod detail panel (B7b — §11) -->
          <aside
            v-if="mods.selectedDetail"
            class="detail-panel"
          >
            <header class="detail-panel__head">
              <b>{{ mods.selectedDetail.name ?? mods.selectedDetail.filename }}</b>
              <button
                class="btn"
                type="button"
                @click="mods.selectDetail('', null)"
              >
                ×
              </button>
            </header>
            <div class="detail-panel__row">
              <span>{{ t('mods.file') }}</span><code>{{ mods.selectedDetail.filename }}</code>
            </div>
            <div class="detail-panel__row">
              <span>{{ t('mods.mod_id') }}</span><code>{{ mods.selectedDetail.modId ?? '—' }}</code>
            </div>
            <div class="detail-panel__row">
              <span>{{ t('common.version') }}</span><code>{{ mods.selectedDetail.version ?? '—' }}</code>
            </div>
            <div class="detail-panel__row">
              <span>{{ t('mods.loader') }}</span><code>{{ mods.selectedDetail.loader ?? '—' }}</code>
            </div>
            <div
              v-if="Object.keys(mods.selectedDetail.depends).length > 0"
              class="detail-panel__deps"
            >
              <h4>{{ t('mods.requires') }}</h4>
              <div
                v-for="(spec, depId) in mods.selectedDetail.depends"
                :key="depId"
                class="detail-panel__dep"
              >
                <code>{{ depId }}</code><span>{{ spec }}</span>
              </div>
            </div>
            <div
              v-if="Object.keys(mods.selectedDetail.breaks).length > 0"
              class="detail-panel__deps detail-panel__deps--breaks"
            >
              <h4>{{ t('mods.breaks') }}</h4>
              <div
                v-for="(spec, depId) in mods.selectedDetail.breaks"
                :key="depId"
                class="detail-panel__dep"
              >
                <code>{{ depId }}</code><span>{{ spec }}</span>
              </div>
            </div>
            <p
              v-if="mods.selectedDetail.readable === false"
              class="mods__warn-text"
            >
              {{ t('mods.unreadable_warn') }}
            </p>
          </aside>
        </div>
      </div>

      <!-- Discover -->
      <div
        v-else-if="mods.tab === 'discover'"
        class="mods__panel"
      >
        <form
          class="mods__search"
          @submit.prevent="onSearch"
        >
          <input
            v-model="searchQuery"
            class="mods__search-input"
            type="text"
            :placeholder="t('mods.search_placeholder')"
          >
          <button
            class="btn"
            type="submit"
            :disabled="mods.searching"
          >
            {{ mods.searching ? t('mods.searching') : t('mods.search') }}
          </button>
        </form>
        <p
          v-if="mods.hits.length === 0 && !mods.searching"
          class="mods__hint"
        >
          {{ t('mods.search_hint', { ctx: instanceCtx }) }}
        </p>
        <ul
          v-else
          class="mod-list"
        >
          <li
            v-for="hit in mods.hits"
            :key="hit.projectId"
            class="mod-list__item"
          >
            <div class="mod-list__info">
              <span class="mod-list__name">{{ hit.title }}</span>
              <span class="mod-list__meta">{{ hit.author ?? '' }} · {{ t('mods.downloads', { n: hit.downloads ?? 0 }) }}</span>
            </div>
            <button
              class="btn btn--primary"
              type="button"
              @click="onInstall(hit.projectId, hit.title)"
            >
              {{ t('mods.install') }}
            </button>
          </li>
        </ul>
      </div>

      <!-- Security §164 -->
      <div
        v-else
        class="mods__panel"
      >
        <div class="mods__actions">
          <button
            class="btn"
            type="button"
            :disabled="mods.scanning"
            @click="onScan"
          >
            {{ mods.scanning ? t('mods.scanning') : t('mods.scan_instance') }}
          </button>
          <button
            class="btn"
            type="button"
            :disabled="mods.fixing"
            @click="onAutofix"
          >
            {{ mods.fixing ? t('mods.fixing') : t('mods.autofix') }}
          </button>
        </div>

        <!-- Health issues §162 -->
        <div
          v-if="mods.health && mods.health.issues.length > 0"
          class="issue-block"
        >
          <h3 class="issue-block__title">
            {{ t('mods.compat_issues') }}
          </h3>
          <div
            v-for="issue in mods.health.issues"
            :key="`${issue.mod}-${issue.kind}`"
            class="issue-block__row"
          >
            <b>{{ issue.mod }}</b>
            <span class="issue-block__kind">{{ issue.kind }}</span>
            <span class="mods__hint">{{ issue.detail }}</span>
            <span
              v-if="issue.fixable"
              class="issue-block__fix"
            >{{ t('mods.fixable', { s: issue.suggestion ?? '' }) }}</span>
          </div>
        </div>

        <!-- Scan results -->
        <div
          v-if="mods.scan"
          class="issue-block"
        >
          <h3 class="issue-block__title">
            {{ t('mods.scan_title', { safe: mods.scan.safe, susp: mods.scan.suspicious, dang: mods.scan.dangerous }) }}
          </h3>
          <p class="mods__hint">
            {{ t('mods.scan_note') }}
          </p>
          <div
            v-for="report in mods.scan.results"
            :key="report.file"
            class="issue-block__row"
          >
            <b>{{ report.file }}</b>
            <span
              class="verdict"
              :class="verdictClass(report.verdict)"
            >{{ report.verdict }}</span>
            <span class="mods__hint">{{ t('mods.score_findings', { score: report.score, n: report.findings.length }) }}</span>
            <span
              v-if="report.quarantined"
              class="issue-block__fix"
            >{{ t('mods.quarantined') }}</span>
          </div>
        </div>

        <!-- Quarantine vault -->
        <div
          v-if="mods.quarantine.length > 0"
          class="issue-block"
        >
          <h3 class="issue-block__title">
            {{ t('mods.quarantine', { n: mods.quarantine.length }) }}
          </h3>
          <div
            v-for="entry in mods.quarantine"
            :key="entry.quarantineFile"
            class="issue-block__row"
          >
            <b>{{ entry.originalName }}</b>
            <span
              class="verdict"
              :class="verdictClass(entry.verdict)"
            >{{ entry.verdict }}</span>
            <button
              class="btn"
              type="button"
              @click="mods.restoreFromQuarantine(entry.quarantineFile)"
            >
              {{ t('mods.restore') }}
            </button>
            <button
              class="btn btn--danger"
              type="button"
              @click="mods.deleteFromQuarantine(entry.quarantineFile)"
            >
              {{ t('common.delete') }}
            </button>
          </div>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.mods {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 960px;
}
.mods__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.mods__title {
  margin: 0;
  font-size: 22px;
}
.mods__instance {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-3);
}
.mods__appselect {
  width: 280px;
}
.mods__state {
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
.mods__state--warn { border-color: rgba(245, 189, 79, 0.4); }
.mods__state--error { border-color: rgba(255, 92, 102, 0.45); }
.mods__hint {
  color: var(--text-3);
  font-size: 12px;
}
.mods__warn-text { color: var(--warning); }
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */
.mods__tabs {
  display: flex;
  gap: 6px;
}
.mods__tab {
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  color: var(--text-2);
  border-radius: 8px;
  padding: 7px 16px;
  cursor: pointer;
  font-size: 12.5px;
  display: flex;
  align-items: center;
  gap: 6px;
}
.mods__tab--active {
  border-color: rgba(255, 92, 71, 0.4);
  color: var(--text-1);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light),
    0 0 12px var(--accent-glow);
}
.mods__tab-badge {
  background: var(--danger);
  color: #fff;
  font-size: 10px;
  border-radius: 8px;
  padding: 0 6px;
}
.mods__panel {
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.mod-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.mod-list__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  background: var(--neu-surface-inset);
  border: 1px solid rgba(0, 0, 0, 0.18);
  box-shadow:
    inset 2px 2px 6px var(--neu-dark-strong),
    inset -2px -2px 6px var(--neu-light);
  border-radius: var(--radius-md);
  padding: 10px 12px;
}
.mod-list__info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.mod-list__name {
  font-size: 13px;
  color: var(--text-1);
  font-weight: 600;
}
.mod-list__meta {
  font-size: 11px;
  color: var(--text-3);
}
.installed-layout {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}
.installed-layout .mod-list {
  flex: 1;
}
.mod-list__item--selected {
  border-color: var(--accent);
}
.mod-list__select {
  all: unset;
  cursor: pointer;
  flex: 1;
}
.modpack-bar {
  border: 1px solid rgba(98, 168, 255, 0.4);
  border-radius: var(--radius-lg);
  /* Frosted solid — không backdrop-filter (perf §5.3) */
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(255, 255, 255, 0) 42%),
    var(--glass-bg-solid);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 12px 32px var(--neu-dark);
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.modpack-bar--done {
  border-color: rgba(80, 216, 144, 0.45);
}
.modpack-bar--failed {
  border-color: rgba(255, 92, 102, 0.45);
}
.modpack-bar__head {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
}
.modpack-bar__state {
  color: var(--text-3);
  font-size: 12px;
  flex: 1;
}
.modpack-bar__track {
  height: 6px;
  background: rgba(0, 0, 0, 0.3);
  box-shadow: inset 1px 1px 3px var(--neu-dark-strong);
  border-radius: 3px;
  overflow: hidden;
}
.modpack-bar__fill {
  height: 100%;
  background: linear-gradient(90deg, var(--accent) 0%, var(--info) 100%);
  transition: width 220ms ease;
}
@media (prefers-reduced-motion: reduce) {
  .modpack-bar__fill { transition: none; }
}
.mrpack-form {
  display: flex;
  gap: 8px;
}
.mrpack-form__input {
  flex: 1;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  border-radius: 8px;
  color: var(--text-1);
  padding: 8px 10px;
  font-size: 13px;
}
.detail-panel {
  width: 280px;
  flex-shrink: 0;
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-lg);
  /* Frosted solid — không backdrop-filter (perf §5.3) */
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(255, 255, 255, 0) 42%),
    var(--glass-bg-solid);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 10px 26px var(--neu-dark);
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  font-size: 12px;
}
.detail-panel__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 13px;
}
.detail-panel__row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  color: var(--text-3);
}
.detail-panel__row code {
  color: var(--text-1);
  font-size: 11px;
  text-align: right;
  word-break: break-all;
}
.detail-panel__deps {
  border-top: 1px solid var(--border-1);
  padding-top: 6px;
}
.detail-panel__deps h4 {
  margin: 0 0 4px;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: var(--text-3);
}
.detail-panel__dep {
  display: flex;
  justify-content: space-between;
  padding: 2px 0;
}
.detail-panel__dep code {
  color: var(--info);
  font-size: 11px;
}
.detail-panel__dep span {
  color: var(--text-3);
  font-size: 11px;
}
.detail-panel__deps--breaks h4 {
  color: var(--danger);
}
.mods__search {
  display: flex;
  gap: 8px;
}
.mods__search-input {
  flex: 1;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  border-radius: 8px;
  color: var(--text-1);
  padding: 8px 10px;
  font-size: 13px;
}
.btn {
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  color: var(--text-2);
  border-radius: 8px;
  padding: 7px 14px;
  cursor: pointer;
  font-size: 12px;
}
.btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.btn--primary {
  border-color: var(--accent);
  color: var(--accent);
}
.btn--danger {
  border-color: var(--danger);
  color: var(--danger);
}
.mods__actions {
  display: flex;
  gap: 10px;
}
.issue-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.issue-block__title {
  margin: 0;
  font-size: 13px;
  color: var(--text-1);
}
.issue-block__row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12.5px;
  background: var(--neu-surface-inset);
  border: 1px solid rgba(0, 0, 0, 0.18);
  border-radius: var(--radius-sm);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light);
  padding: 8px 11px;
  color: var(--text-2);
  flex-wrap: wrap;
}
.issue-block__kind {
  font-size: 11px;
  color: var(--warning);
}
.issue-block__fix {
  font-size: 11px;
  color: var(--info);
}
.verdict {
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.1em;
  border-radius: 4px;
  padding: 2px 6px;
}
.verdict--ok {
  color: var(--success);
  background: rgba(80, 216, 144, 0.12);
}
.verdict--warn {
  color: var(--warning);
  background: rgba(245, 189, 79, 0.12);
}
.verdict--danger {
  color: var(--danger);
  background: rgba(255, 92, 102, 0.12);
}
</style>
