<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'

import AppSelect, { type AppSelectOption } from '@/app/components/AppSelect.vue'
import { t } from '@/shared/i18n'
import { useAssetsStore } from '@/stores/assets.store'
import { useInstancesStore } from '@/stores/instances.store'
import { useNotificationsStore } from '@/stores/notifications.store'
import { useResourcesStore } from '@/stores/resources.store'
import { ASSET_CATEGORIES } from '@/types/assets'
import type { AssetEntry } from '@/types/assets'
import type { AssetSort } from '@/stores/assets.store'
import type { ResourceProject } from '@/types/assets'
import type { BuildEntry, ValidationFinding } from '@/types/resources'

type PageTab = 'assets' | 'packs'

const assets = useAssetsStore()
const resources = useResourcesStore()
const instances = useInstancesStore()
const notifications = useNotificationsStore()

const tab = ref<PageTab>('assets')
const selectedAssetId = ref<string | null>(null)
const importCategory = ref('item')
const fileInput = ref<HTMLInputElement | null>(null)

onMounted(() => {
  void assets.load()
  void instances.load()
  void resources.load()
})

const selectedAsset = computed(() =>
  assets.items.find((a) => a.id === selectedAssetId.value) ?? null,
)

const selectedPreview = computed(() =>
  selectedAssetId.value ? assets.previews[selectedAssetId.value] ?? null : null,
)

async function onSelectAsset(asset: AssetEntry): Promise<void> {
  selectedAssetId.value = asset.id
  await assets.loadPreview(asset.id)
}

function onPickFile(): void {
  fileInput.value?.click()
}

async function onFileChosen(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = '' // cho chọn lại cùng file
  if (!file) return
  const ok = await assets.importPng(file, importCategory.value)
  if (ok) {
    notifications.push('success', t('rs.notif_imported'), file.name, 'resources')
  } else {
    notifications.push('error', t('rs.notif_import_failed'), `${assets.errorCode}: ${assets.errorMessage}`, 'resources')
  }
}

async function onDelete(asset: AssetEntry): Promise<void> {
  const ok = await assets.remove(asset.id)
  if (ok) {
    if (selectedAssetId.value === asset.id) selectedAssetId.value = null
    notifications.push('success', t('rs.notif_deleted'), asset.name, 'resources')
  } else {
    notifications.push('error', t('rs.notif_delete_failed'), `${assets.errorCode}: ${assets.errorMessage}`, 'resources')
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`
}

// ---------------------------------------------------------------------------
// AppSelect options + handlers (thay native select — dropdown dark premium)
// ---------------------------------------------------------------------------

const categoryOptions: AppSelectOption[] = ASSET_CATEGORIES.map((cat) => ({ value: cat, label: cat }))
const sortOptions = computed<AppSelectOption[]>(() => [
  { value: 'newest', label: t('rs.sort_newest') },
  { value: 'oldest', label: t('rs.sort_oldest') },
  { value: 'name', label: t('rs.sort_name') },
  { value: 'size', label: t('rs.sort_size') },
])

function onCategoryChange(category: string): void {
  assets.setFilter({ category })
}

function onSortChange(sort: string): void {
  assets.setFilter({ sort: sort as AssetSort })
}

function onImportCategoryChange(value: string): void {
  importCategory.value = value
}

const projectOptions = computed(() => [
  { value: '', label: t('rs.project_select') },
  ...assets.projects.map((p) => ({ value: p.id, label: `${p.name} (${p.minecraft?.version ?? '—'})` })),
])

function onProjectPick(id: string): void {
  assets.selectProject(id || null)
}

const installInstanceOptions = computed(() => [
  { value: '', label: t('rs.instance_select') },
  ...instances.items.map((i) => ({ value: i.id, label: `${i.name} (${i.minecraftVersion})` })),
])

function onInstallInstanceChange(id: string): void {
  installInstanceId.value = id
}

const layerInstanceOptions = computed<AppSelectOption[]>(() => [
  { value: '', label: t('rs.instance_select') },
  ...instances.items.map((i) => ({ value: i.id, label: i.name })),
])

function onLayerInstanceChange(id: string): void {
  layerInstanceId.value = id
}

const wizardVersionOptions = computed(() =>
  (resources.wizard?.versions ?? []).map((v) => ({ value: v, label: v })),
)

function onWizardVersionChange(v: string): void {
  wizardVersion.value = v
}

const wizardTemplateOptions = computed(() =>
  (resources.wizard?.templates ?? []).map((tpl) => ({
    value: tpl.id,
    label: `${tpl.labelKey} — ${
      Object.entries(tpl.modules)
        .filter(([, on]) => on)
        .map(([k]) => k)
        .join(', ') || t('rs.template_empty')
    }`,
  })),
)

function onWizardTemplateChange(id: string): void {
  wizardTemplate.value = id
}

/** Assign flow — target input hiện tại để nhập path textures. */
const assignTarget = ref('')
const assignBusy = computed(() => assets.busyId === selectedAssetId.value)

async function onAssign(): Promise<void> {
  const projectId = assets.selectedProjectId
  if (!projectId || !selectedAssetId.value || !assignTarget.value.trim()) return
  const ok = await assets.assign(selectedAssetId.value, projectId, assignTarget.value.trim())
  if (ok) {
    notifications.push(
      'success',
      t('rs.notif_assigned'),
      `${selectedAsset.value?.name} → ${assignTarget.value}`,
      'resources',
    )
    assignTarget.value = ''
  } else {
    notifications.push('error', t('rs.notif_assign_failed'), `${assets.errorCode}: ${assets.errorMessage}`, 'resources')
  }
}

// ---------------------------------------------------------------------------
// B8b — Packs tab
// ---------------------------------------------------------------------------

/** Wizard create-pack (§10.2): name + version + template + description. */
const wizardName = ref('')
const wizardVersion = ref('')
const wizardTemplate = ref('minimal')
const wizardDescription = ref('')

async function onWizardOpen(): Promise<void> {
  resources.openWizard()
  wizardName.value = ''
  wizardDescription.value = ''
  // default version set sau khi wizard info load
  setTimeout(() => {
    if (!wizardVersion.value && resources.wizard) {
      wizardVersion.value = resources.wizard.defaultVersion
    }
  }, 0)
}

async function onWizardCreate(): Promise<void> {
  const ok = await resources.createProject({
    name: wizardName.value.trim(),
    mcVersion: wizardVersion.value || resources.wizard?.defaultVersion || '1.21.4',
    template: wizardTemplate.value,
    description: wizardDescription.value.trim(),
  })
  if (ok) {
    notifications.push('success', t('rs.notif_created'), wizardName.value, 'resources')
  } else {
    notifications.push('error', t('rs.notif_create_failed'), `${resources.errorCode}: ${resources.errorMessage}`, 'resources')
  }
}

async function onSelectProject(project: ResourceProject): Promise<void> {
  resources.selectProject(project.id)
  await resources.refreshBuilds(project.id)
}

async function onValidate(): Promise<void> {
  const project = resources.selectedProject
  if (!project) return
  const ok = await resources.runValidation(project.id)
  notifications.push(
    ok ? 'success' : 'warning',
    ok ? t('rs.notif_validation_ok') : t('rs.notif_validation_fail'),
    ok
      ? t('rs.notif_warnings', { n: resources.warningCount })
      : `${t('rs.notif_errors', { n: resources.errorCount })}, ${t('rs.notif_warnings', { n: resources.warningCount })}`,
    'resources',
  )
}

async function onGenerate(): Promise<void> {
  const project = resources.selectedProject
  if (!project) return
  const ok = await resources.regenerate(project.id)
  if (ok) notifications.push('success', t('rs.notif_generated'), project.name, 'resources')
  else notifications.push('error', t('rs.notif_generate_failed'), `${resources.errorCode}: ${resources.errorMessage}`, 'resources')
}

async function onBuild(): Promise<void> {
  const project = resources.selectedProject
  if (!project) return
  const ok = await resources.build(project.id)
  if (ok && resources.lastManifest) {
    notifications.push(
      'success',
      t('rs.notif_built'),
      `${resources.lastManifest.file} (${formatBytes(resources.lastManifest.bytes)})`,
      'resources',
    )
  } else {
    notifications.push('error', t('rs.notif_build_failed'), `${resources.errorCode}: ${resources.errorMessage}`, 'resources')
  }
}

const installInstanceId = ref('')
const installOverwrite = ref(false)

async function onInstall(): Promise<void> {
  const project = resources.selectedProject
  if (!project || !installInstanceId.value) return
  const ok = await resources.installToInstance(project.id, installInstanceId.value, installOverwrite.value)
  if (ok) {
    notifications.push('success', t('rs.notif_installed'), `${project.name} → ${installInstanceId.value}`, 'resources')
  } else {
    notifications.push('error', t('rs.notif_install_failed'), `${resources.errorCode}: ${resources.errorMessage}`, 'resources')
  }
}

async function onRemoveProject(project: ResourceProject): Promise<void> {
  const ok = await resources.removeProject(project.id)
  if (ok) notifications.push('success', t('rs.notif_pack_deleted'), project.name, 'resources')
  else notifications.push('error', t('rs.notif_delete_failed'), `${resources.errorCode}: ${resources.errorMessage}`, 'resources')
}

function findingClass(finding: ValidationFinding): string {
  return finding.severity === 'ERROR' ? 'finding--error' : 'finding--warning'
}

function buildLabel(build: BuildEntry): string {
  const d = new Date(build.builtAt * 1000)
  return `${build.file} · ${formatBytes(build.bytes)} · ${d.toLocaleString()}`
}

// Layer panel (§173)
const layerInstanceId = ref('')

async function onLoadLayer(): Promise<void> {
  if (!layerInstanceId.value) return
  await resources.loadLayer(layerInstanceId.value)
}

async function onLayerMove(filename: string, delta: number): Promise<void> {
  const ok = await resources.movePack(layerInstanceId.value, filename, delta)
  if (!ok) {
    notifications.push('error', t('rs.notif_reorder_failed'), `${resources.errorCode}: ${resources.errorMessage}`, 'resources')
  }
}
</script>

<template>
  <section class="rs">
    <header class="rs__header">
      <h1 class="rs__title">
        {{ t('rs.title') }}
      </h1>
      <div class="rs__header-actions">
        <div
          class="rs__tabs"
          role="tablist"
        >
          <button
            class="rs__tab"
            :class="{ 'rs__tab--active': tab === 'assets' }"
            type="button"
            role="tab"
            :aria-selected="tab === 'assets'"
            @click="tab = 'assets'"
          >
            {{ t('rs.tab_assets') }}
          </button>
          <button
            class="rs__tab"
            :class="{ 'rs__tab--active': tab === 'packs' }"
            type="button"
            role="tab"
            :aria-selected="tab === 'packs'"
            @click="tab = 'packs'"
          >
            {{ t('rs.tab_packs') }}
          </button>
        </div>
        <button
          class="rs-btn"
          type="button"
          :disabled="assets.phase === 'loading'"
          @click="assets.load()"
        >
          {{ t('common.reload') }}
        </button>
      </div>
    </header>

    <!-- Loading §158 -->
    <div
      v-if="assets.phase === 'loading'"
      class="rs__state"
    >
      <div
        v-for="n in 4"
        :key="n"
        class="skeleton"
      />
    </div>

    <!-- Offline §68 -->
    <div
      v-else-if="assets.phase === 'offline'"
      class="rs__state rs__state--warn"
    >
      <p>{{ t('rs.offline') }}</p>
      <button
        class="rs-btn"
        type="button"
        @click="assets.load()"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <!-- Error §160 -->
    <div
      v-else-if="assets.phase === 'error'"
      class="rs__state rs__state--error"
    >
      <p>{{ t('rs.error') }}</p>
      <p class="rs__hint">
        {{ assets.errorCode }}: {{ assets.errorMessage }}
      </p>
      <button
        class="rs-btn"
        type="button"
        @click="assets.load()"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <!-- Editor shell §12/§123: Assets | Preview | Inspector -->
    <div
      v-else-if="tab === 'assets'"
      class="rs__workspace"
    >
      <!-- Cột 1: Asset explorer + import -->
      <div class="rs__col rs__col--assets">
        <div class="rs__filters">
          <input
            class="rs__search"
            type="text"
            :placeholder="t('rs.search')"
            :value="assets.query"
            @change="assets.setFilter({ query: ($event.target as HTMLInputElement).value })"
          >
          <AppSelect
            class="rs__appselect"
            :model-value="assets.category"
            :options="categoryOptions"
            :aria-label="t('rs.category')"
            @update:model-value="onCategoryChange"
          />
          <AppSelect
            class="rs__appselect"
            :model-value="assets.sort"
            :options="sortOptions"
            :aria-label="t('rs.sort')"
            @update:model-value="onSortChange"
          />
        </div>

        <div class="rs__import">
          <AppSelect
            :model-value="importCategory"
            :options="categoryOptions"
            :aria-label="t('rs.import_category')"
            class="rs__appselect rs__appselect--import"
            @update:model-value="onImportCategoryChange"
          />
          <button
            class="rs-btn rs-btn--primary"
            type="button"
            :disabled="assets.importing"
            @click="onPickFile"
          >
            {{ assets.importing ? t('rs.importing') : t('rs.import_png') }}
          </button>
          <input
            ref="fileInput"
            type="file"
            accept="image/png"
            hidden
            @change="onFileChosen"
          >
        </div>

        <div
          v-if="assets.items.length === 0"
          class="rs__empty"
        >
          {{ t('rs.empty') }}
        </div>
        <ul
          v-else
          class="asset-grid"
        >
          <li
            v-for="asset in assets.items"
            :key="asset.id"
            class="asset-grid__item"
            :class="{ 'asset-grid__item--selected': asset.id === selectedAssetId }"
          >
            <button
              type="button"
              class="asset-grid__btn"
              @click="onSelectAsset(asset)"
            >
              <img
                v-if="assets.previews[asset.id]"
                class="asset-grid__thumb"
                :src="assets.previews[asset.id]"
                :alt="asset.name"
                loading="lazy"
              >
              <span
                v-else
                class="asset-grid__thumb asset-grid__thumb--ph"
              >▾</span>
              <span class="asset-grid__name">{{ asset.name }}</span>
              <span class="asset-grid__meta">{{ asset.width }}×{{ asset.height }} · {{ formatBytes(asset.bytes) }}</span>
            </button>
          </li>
        </ul>
      </div>

      <!-- Cột 2: Preview canvas -->
      <div class="rs__col rs__col--preview">
        <div
          v-if="assets.previewLoadingId"
          class="rs__hint"
        >
          {{ t('rs.loading_preview') }}
        </div>
        <template v-else-if="selectedAsset">
          <img
            v-if="selectedPreview"
            class="rs__preview-img"
            :src="selectedPreview"
            :alt="selectedAsset.name"
          >
          <div
            v-else
            class="rs__hint"
          >
            {{ t('rs.no_preview') }}
          </div>
          <div class="rs__preview-meta">
            {{ selectedAsset.width }}×{{ selectedAsset.height }} · {{ selectedAsset.mime }} ·
            {{ formatBytes(selectedAsset.bytes) }}
          </div>
          <code class="rs__hash">{{ selectedAsset.sha256.slice(0, 16) }}…</code>
        </template>
        <div
          v-else
          class="rs__hint"
        >
          {{ t('rs.pick_hint') }}
        </div>
      </div>

      <!-- Cột 3: Inspector — assign vào project -->
      <div class="rs__col rs__col--inspector">
        <h2 class="rs__subtitle">
          {{ t('rs.inspector') }}
        </h2>
        <template v-if="selectedAsset">
          <div class="rs__inspector-row">
            <span>{{ t('rs.name') }}</span>
            <code>{{ selectedAsset.name }}</code>
          </div>
          <div class="rs__inspector-row">
            <span>{{ t('rs.category') }}</span>
            <code>{{ selectedAsset.category }}</code>
          </div>
          <div class="rs__inspector-row">
            <span>{{ t('rs.tags') }}</span>
            <code>{{ selectedAsset.tags.join(', ') || '—' }}</code>
          </div>
          <div class="rs__inspector-row">
            <span>{{ t('rs.source') }}</span>
            <code>{{ selectedAsset.source }}</code>
          </div>

          <label class="rs__field">
            <span>{{ t('rs.target_project') }}</span>
            <AppSelect
              :model-value="assets.selectedProjectId ?? ''"
              :options="projectOptions"
              :aria-label="t('rs.target_project')"
              @update:model-value="onProjectPick"
            />
          </label>

          <label class="rs__field">
            <span>{{ t('rs.target_path') }}</span>
            <input
              v-model="assignTarget"
              class="rs__search"
              type="text"
              placeholder="assets/minecraft/textures/item/sword.png"
            >
          </label>
          <button
            class="rs-btn rs-btn--primary"
            type="button"
            :disabled="assignBusy || !assets.selectedProjectId || !assignTarget.trim()"
            @click="onAssign"
          >
            {{ assignBusy ? t('rs.assigning') : t('rs.assign') }}
          </button>
          <p class="rs__hint">
            {{ t('rs.path_hint') }}
          </p>

          <button
            class="rs-btn rs-btn--danger"
            type="button"
            :disabled="assignBusy"
            @click="onDelete(selectedAsset)"
          >
            {{ t('rs.delete_asset') }}
          </button>
        </template>
        <div
          v-else
          class="rs__hint"
        >
          {{ t('rs.select_inspector') }}
        </div>
      </div>
    </div>

    <!-- B8b — Packs tab: lifecycle create → validate → build → install → layer -->
    <template v-else>
      <div class="rs__packs-bar">
        <button
          class="rs-btn rs-btn--primary"
          type="button"
          @click="onWizardOpen"
        >
          {{ t('rs.new_pack') }}
        </button>
        <span class="rs__hint">
          {{ t('rs.projects_count', { n: resources.projects.length }) }}
        </span>
      </div>

      <div class="rs__packs">
        <!-- Cột 1: project list -->
        <div class="rs__col rs__col--projects">
          <h2 class="rs__subtitle">
            {{ t('rs.projects') }}
          </h2>
          <div
            v-if="resources.phase === 'ready' && resources.projects.length === 0"
            class="rs__empty"
          >
            {{ t('rs.no_packs') }}
          </div>
          <ul
            v-else
            class="pack-list"
          >
            <li
              v-for="project in resources.projects"
              :key="project.id"
            >
              <button
                type="button"
                class="pack-list__item"
                :class="{ 'pack-list__item--selected': project.id === resources.selectedProjectId }"
                @click="onSelectProject(project)"
              >
                <span class="pack-list__name">{{ project.name }}</span>
                <span class="pack-list__meta">MC {{ project.minecraft?.version }} · {{ project.buildCount ?? 0 }} build(s)</span>
              </button>
            </li>
          </ul>
        </div>

        <!-- Cột 2: lifecycle actions cho project đang chọn -->
        <div class="rs__col rs__col--lifecycle">
          <h2 class="rs__subtitle">
            {{ t('rs.lifecycle') }}
          </h2>
          <template v-if="resources.selectedProject">
            <div class="rs__inspector-row">
              <span>{{ t('rs.project_label') }}</span>
              <code>{{ resources.selectedProject.name }}</code>
            </div>
            <div class="rs__inspector-row">
              <span>{{ t('rs.mc_version') }}</span>
              <code>{{ resources.selectedProject.minecraft?.version }}</code>
            </div>

            <div class="rs__actions">
              <button
                class="rs-btn"
                type="button"
                :disabled="resources.generating || resources.busyProjectId === resources.selectedProjectId"
                @click="onGenerate"
              >
                {{ resources.generating ? t('rs.generating') : t('rs.regenerate') }}
              </button>
              <button
                class="rs-btn"
                type="button"
                :disabled="resources.validating || resources.busyProjectId === resources.selectedProjectId"
                @click="onValidate"
              >
                {{ resources.validating ? t('rs.validating') : t('rs.validate') }}
              </button>
              <button
                class="rs-btn rs-btn--primary"
                type="button"
                :disabled="resources.building || resources.busyProjectId === resources.selectedProjectId"
                @click="onBuild"
              >
                {{ resources.building ? t('rs.building') : t('rs.build_zip') }}
              </button>
            </div>

            <!-- Findings panel §127 -->
            <div
              v-if="resources.validation"
              class="findings"
            >
              <div class="findings__head">
                <span :class="resources.validation.ok ? 'finding--ok' : 'finding--error'">
                  {{ resources.validation.ok ? 'PASS' : 'FAIL' }}
                </span>
                <span class="rs__hint">pack_format {{ resources.validation.packFormat }}</span>
              </div>
              <div
                v-if="resources.validation.findings.length === 0"
                class="rs__hint"
              >
                {{ t('rs.no_findings') }}
              </div>
              <div
                v-for="(finding, i) in resources.validation.findings"
                v-else
                :key="`${finding.path}-${i}`"
                class="finding"
                :class="findingClass(finding)"
              >
                <span class="finding__sev">{{ finding.severity }}</span>
                <span class="finding__body">
                  <code>{{ finding.path }}</code> — {{ finding.code }}: {{ finding.detail }}
                </span>
              </div>
            </div>

            <!-- Builds list -->
            <div class="rs__field">
              <span>{{ t('rs.builds', { n: resources.builds.length }) }}</span>
              <div
                v-if="resources.builds.length === 0"
                class="rs__hint"
              >
                {{ t('rs.no_builds') }}
              </div>
              <ul
                v-else
                class="build-list"
              >
                <li
                  v-for="build in resources.builds"
                  :key="build.file"
                  class="build-list__item"
                >
                  <code>{{ buildLabel(build) }}</code>
                </li>
              </ul>
            </div>

            <!-- Install panel §75 -->
            <div class="rs__field">
              <span>{{ t('rs.install_into') }}</span>
              <AppSelect
                :model-value="installInstanceId"
                :options="installInstanceOptions"
                :aria-label="t('rs.install_aria')"
                @update:model-value="onInstallInstanceChange"
              />
              <label class="rs__check">
                <input
                  v-model="installOverwrite"
                  type="checkbox"
                >
                {{ t('rs.overwrite') }}
              </label>
              <button
                class="rs-btn rs-btn--primary"
                type="button"
                :disabled="!installInstanceId || resources.busyProjectId === resources.selectedProjectId"
                @click="onInstall"
              >
                {{ resources.busyProjectId === resources.selectedProjectId ? t('rs.installing') : t('rs.install_pack') }}
              </button>
            </div>

            <button
              class="rs-btn rs-btn--danger"
              type="button"
              :disabled="resources.busyProjectId === resources.selectedProjectId"
              @click="onRemoveProject(resources.selectedProject)"
            >
              {{ t('rs.delete_project') }}
            </button>
          </template>
          <div
            v-else
            class="rs__hint"
          >
            {{ t('rs.select_project_hint') }}
          </div>
        </div>

        <!-- Cột 3: layer panel §173 -->
        <div class="rs__col rs__col--layer">
          <h2 class="rs__subtitle">
            {{ t('rs.layer_order') }}
          </h2>
          <label class="rs__field">
            <span>{{ t('common.instance') }}</span>
            <AppSelect
              :model-value="layerInstanceId"
              :options="layerInstanceOptions"
              :aria-label="t('rs.layer_aria')"
              @update:model-value="onLayerInstanceChange"
            />
          </label>
          <button
            class="rs-btn"
            type="button"
            :disabled="!layerInstanceId || resources.layerLoading"
            @click="onLoadLayer"
          >
            {{ resources.layerLoading ? t('common.loading') : t('rs.load_layer') }}
          </button>

          <div
            v-if="resources.layer"
            class="layer"
          >
            <div class="rs__hint">
              {{ t('rs.layer_hint') }}
            </div>
            <ul class="layer__list">
              <li
                v-for="(pack, i) in resources.layer.order"
                :key="pack"
                class="layer__item"
              >
                <code>{{ pack }}</code>
                <span class="layer__btns">
                  <button
                    class="rs-btn rs-btn--mini"
                    type="button"
                    :disabled="i === 0"
                    :aria-label="t('rs.up_aria')"
                    @click="onLayerMove(pack, -1)"
                  >
                    ↑
                  </button>
                  <button
                    class="rs-btn rs-btn--mini"
                    type="button"
                    :disabled="i === resources.layer!.order.length - 1"
                    :aria-label="t('rs.down_aria')"
                    @click="onLayerMove(pack, 1)"
                  >
                    ↓
                  </button>
                </span>
              </li>
            </ul>
            <div
              v-if="resources.layer.preview.conflicts.length > 0"
              class="layer__conflicts"
            >
              <span class="rs__hint">{{ t('rs.conflicts', { n: resources.layer.preview.conflicts.length }) }}</span>
              <div
                v-for="conflict in resources.layer.preview.conflicts.slice(0, 10)"
                :key="conflict.path"
                class="finding finding--warning"
              >
                <span class="finding__body">
                  <code>{{ conflict.path }}</code> — {{ conflict.packs.join(' vs ') }}
                </span>
              </div>
            </div>
          </div>
          <div
            v-else
            class="rs__hint"
          >
            {{ t('rs.layer_select_hint') }}
          </div>
        </div>
      </div>
    </template>

    <!-- Wizard modal §10.2 -->
    <div
      v-if="resources.wizardOpen"
      class="wizard-backdrop"
      @click.self="resources.closeWizard()"
    >
      <div
        class="wizard"
        role="dialog"
        aria-modal="true"
        :aria-label="t('rs.wizard_aria')"
      >
        <h2 class="rs__subtitle">
          {{ t('rs.wizard_title') }}
        </h2>
        <div
          v-if="!resources.wizard"
          class="rs__hint"
        >
          {{ t('rs.wizard_loading') }}
        </div>
        <template v-else>
          <label class="rs__field">
            <span>{{ t('rs.name') }}</span>
            <input
              v-model="wizardName"
              class="rs__search"
              type="text"
              :placeholder="t('rs.name_ph')"
            >
          </label>
          <label class="rs__field">
            <span>{{ t('rs.mc_version_full') }}</span>
            <AppSelect
              :model-value="wizardVersion"
              :options="wizardVersionOptions"
              :aria-label="t('rs.mc_version_full')"
              @update:model-value="onWizardVersionChange"
            />
          </label>
          <label class="rs__field">
            <span>{{ t('rs.base_template') }}</span>
            <AppSelect
              :model-value="wizardTemplate"
              :options="wizardTemplateOptions"
              :aria-label="t('rs.base_template')"
              @update:model-value="onWizardTemplateChange"
            />
          </label>
          <label class="rs__field">
            <span>{{ t('rs.wizard_desc') }}</span>
            <input
              v-model="wizardDescription"
              class="rs__search"
              type="text"
              :placeholder="t('rs.desc_ph')"
            >
          </label>
          <div class="wizard__actions">
            <button
              class="rs-btn"
              type="button"
              @click="resources.closeWizard()"
            >
              {{ t('common.cancel') }}
            </button>
            <button
              class="rs-btn rs-btn--primary"
              type="button"
              :disabled="resources.wizardCreating || !wizardName.trim()"
              @click="onWizardCreate"
            >
              {{ resources.wizardCreating ? t('rs.creating') : t('rs.create_pack') }}
            </button>
          </div>
        </template>
      </div>
    </div>
  </section>
</template>

<style scoped>
.rs {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 1200px;
}
.rs__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.rs__header-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}
.rs__tabs {
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
.rs__tab {
  background: transparent;
  border: none;
  color: var(--text-2);
  border-radius: var(--radius-sm);
  padding: 5px 14px;
  cursor: pointer;
  font-size: 12px;
}
.rs__tab--active {
  background: var(--neu-surface);
  color: var(--accent);
  box-shadow:
    2px 2px 5px var(--neu-dark),
    -2px -2px 5px var(--neu-light);
}
.rs__title {
  margin: 0;
  font-size: 22px;
}
.rs__subtitle {
  margin: 0;
  font-size: 14px;
}
.rs__state {
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 22px;
  color: var(--text-2);
  font-size: 13px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-start;
}
.rs__state--warn { border-color: var(--warning); }
.rs__state--error { border-color: var(--danger); }
.rs__hint {
  color: var(--text-3);
  font-size: 12px;
}
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */
.rs__workspace {
  display: grid;
  grid-template-columns: 1.4fr 1fr 0.9fr;
  gap: 12px;
  align-items: start;
}
.rs__col {
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 320px;
}
.rs__filters {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.rs__search,
.rs__appselect {
  width: 200px;
}
.rs__import {
  display: flex;
  gap: 6px;
}
.rs__appselect--import {
  width: auto;
  flex: 1;
}
.rs-btn {
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
.rs-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.rs-btn--primary {
  border-color: var(--accent);
  color: var(--accent);
}
.rs-btn--danger {
  border-color: var(--danger);
  color: var(--danger);
}
.rs-btn--mini {
  padding: 2px 8px;
  font-size: 11px;
}
.rs__empty {
  font-size: 12px;
  color: var(--text-3);
  padding: 12px 0;
}
.asset-grid {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 8px;
  overflow-y: auto;
  max-height: 420px;
}
.asset-grid__item--selected .asset-grid__btn {
  border-color: rgba(255, 92, 71, 0.4);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light),
    0 0 12px var(--accent-glow);
}
.asset-grid__btn {
  all: unset;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  border-radius: 8px;
  padding: 8px;
}
.asset-grid__thumb {
  width: 48px;
  height: 48px;
  object-fit: contain;
  image-rendering: pixelated;
}
.asset-grid__thumb--ph {
  display: grid;
  place-items: center;
  background: var(--surface-3);
  border-radius: 6px;
  color: var(--text-3);
}
.asset-grid__name {
  font-size: 11px;
  color: var(--text-1);
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.asset-grid__meta {
  font-size: 9px;
  color: var(--text-3);
}
.rs__col--preview {
  align-items: center;
  justify-content: center;
}
.rs__preview-img {
  max-width: 100%;
  max-height: 280px;
  object-fit: contain;
  image-rendering: pixelated;
  background:
    repeating-conic-gradient(var(--surface-3) 0% 25%, var(--surface-2) 0% 50%) 0 0 / 16px 16px;
  border-radius: 8px;
}
.rs__preview-meta {
  font-size: 11px;
  color: var(--text-3);
}
.rs__hash {
  font-size: 10px;
  color: var(--info);
}
.rs__inspector-row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--text-3);
}
.rs__inspector-row code {
  color: var(--text-1);
  font-size: 11px;
  text-align: right;
  word-break: break-all;
}
.rs__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  color: var(--text-3);
}
.rs__check {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-3);
}

/* ---- B8b Packs tab ---- */
.rs__packs-bar {
  display: flex;
  align-items: center;
  gap: 12px;
}
.rs__packs {
  display: grid;
  grid-template-columns: 1fr 1.4fr 1fr;
  gap: 12px;
  align-items: start;
}
.pack-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow-y: auto;
  max-height: 420px;
}
.pack-list__item {
  all: unset;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 2px;
  border: 1px solid var(--border-1);
  border-radius: 8px;
  background: var(--surface-2);
  padding: 8px 10px;
}
.pack-list__item--selected {
  border-color: rgba(255, 92, 71, 0.4);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light),
    0 0 12px var(--accent-glow);
}
.pack-list__name {
  font-size: 12px;
  color: var(--text-1);
}
.pack-list__meta {
  font-size: 10px;
  color: var(--text-3);
}
.rs__actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.findings {
  border: 1px solid var(--border-1);
  border-radius: 8px;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 220px;
  overflow-y: auto;
}
.findings__head {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
}
.finding {
  display: flex;
  gap: 6px;
  font-size: 11px;
  border-radius: 6px;
  padding: 4px 6px;
}
.finding--error {
  background: color-mix(in srgb, var(--danger) 12%, transparent);
  color: var(--danger);
}
.finding--warning {
  background: color-mix(in srgb, var(--warning) 12%, transparent);
  color: var(--warning);
}
.finding--ok {
  color: var(--success, var(--accent));
}
.finding__sev {
  font-weight: 600;
  min-width: 52px;
}
.finding__body code {
  font-size: 10px;
  word-break: break-all;
}
.build-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 140px;
  overflow-y: auto;
}
.build-list__item code {
  font-size: 10px;
  color: var(--text-2);
  word-break: break-all;
}
.layer__list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.layer__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  border: 1px solid rgba(0, 0, 0, 0.18);
  border-radius: var(--radius-sm);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light);
  padding: 5px 9px;
}
.layer__item code {
  font-size: 10px;
  color: var(--text-1);
  word-break: break-all;
}
.layer__btns {
  display: flex;
  gap: 4px;
}
.layer__conflicts {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

/* Wizard modal */
.wizard-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(4, 5, 9, 0.5);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
  display: grid;
  place-items: center;
  z-index: 60;
}
.wizard {
  width: min(420px, 92vw);
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
  gap: 12px;
}
.wizard__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
