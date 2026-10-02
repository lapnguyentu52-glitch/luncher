<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import AppSelect, { type AppSelectOption } from '@/app/components/AppSelect.vue'
import { t } from '@/shared/i18n'
import { renderTotemModel } from '@/services/visualsCommands'
import { useInstancesStore } from '@/stores/instances.store'
import { useNotificationsStore } from '@/stores/notifications.store'
import { useVisualsStore, type VisualTab } from '@/stores/visuals.store'
import { specToDocument, VoxelEngine, type RenderQuality } from '@/features/visuals/voxelEngine'

const visuals = useVisualsStore()
const instances = useInstancesStore()
const notifications = useNotificationsStore()

const TABS: Array<{ id: VisualTab; label: () => string }> = [
  { id: 'crosshair', label: () => t('vs.tab_crosshair') },
  { id: 'totem', label: () => t('vs.tab_totem') },
  { id: 'totem3d', label: () => t('vs.tab_totem3d') },
  { id: 'hud', label: () => t('vs.tab_hud') },
  { id: 'fx', label: () => t('vs.tab_fx') },
]

onMounted(() => {
  void instances.load()
  void visuals.load()
})

// §132 — RAF debounce cho preview khi kéo slider
let rafId = 0
function schedulePreview(): void {
  if (rafId) cancelAnimationFrame(rafId)
  rafId = requestAnimationFrame(() => {
    rafId = 0
    void visuals.refreshPreview()
  })
}
onBeforeUnmount(() => {
  if (rafId) cancelAnimationFrame(rafId)
})

const presetsForTab = computed(() => {
  if (visuals.tab === 'crosshair') return visuals.crosshairPresets?.presets ?? []
  if (visuals.tab === 'totem') return visuals.totemPresets?.presets ?? []
  return []
})

const currentSpecEntries = computed<Array<[string, unknown]>>(() => {
  if (visuals.tab === 'crosshair' && visuals.crosshairSpec) {
    return Object.entries(visuals.crosshairSpec)
  }
  if (visuals.tab === 'totem' && visuals.totemSpec) {
    return Object.entries(visuals.totemSpec)
  }
  return []
})

function onPatch(key: string, event: Event): void {
  const el = event.target as HTMLInputElement | HTMLSelectElement
  let value: unknown = el.value
  if (el instanceof HTMLInputElement && el.type === 'checkbox') value = el.checked
  else if (el instanceof HTMLInputElement && el.type === 'range') value = Number(el.value)
  visuals.patchSpec({ [key]: value })
  schedulePreview()
}

function specString(key: string): string {
  const entry = currentSpecEntries.value.find(([k]) => k === key)
  return entry ? String(entry[1] ?? '') : ''
}

function specBool(key: string): boolean {
  const entry = currentSpecEntries.value.find(([k]) => k === key)
  return entry ? Boolean(entry[1]) : false
}

function specNumber(key: string, fallback = 0): number {
  const entry = currentSpecEntries.value.find(([k]) => k === key)
  const v = entry ? Number(entry[1]) : NaN
  return Number.isFinite(v) ? v : fallback
}

// ---- AppSelect options + handlers (thay native select) ----
const shapeOptions: AppSelectOption[] = [
  { value: 'cross', label: 'cross' },
  { value: 'circle', label: 'circle' },
  { value: 'dot', label: 'dot' },
]

function onShapeChange(key: string, value: string): void {
  visuals.patchSpec({ [key]: value })
  schedulePreview()
}

const qualityOptions: AppSelectOption[] = [
  { value: 'low', label: 'LOW' },
  { value: 'medium', label: 'MEDIUM' },
  { value: 'high', label: 'HIGH' },
]

const hitKindOptions = computed<AppSelectOption[]>(() =>
  (visuals.fx?.hit.kinds ?? []).map((k) => ({ value: k, label: k })),
)

function onHitKindChange(v: string): void {
  visuals.patchSpec({ kind: v })
  schedulePreview()
}

const particleShapeOptions = computed<AppSelectOption[]>(() =>
  (visuals.fx?.particle.shapes ?? []).map((s) => ({ value: s, label: s })),
)

function onParticleShapeChange(v: string): void {
  visuals.particleSpec = { ...visuals.particleSpec, shape: v }
  void visuals.refreshParticlePreview()
}

const exportInstanceOptions = computed<AppSelectOption[]>(() => [
  { value: '', label: t('vs.export_none') },
  ...instances.items.map((i) => ({ value: i.id, label: `${i.name} (${i.minecraftVersion})` })),
])

function onExportInstanceChange(v: string): void {
  exportInstanceId.value = v
}

// ---- Export form ----
const exportName = ref('')
const exportMcVersion = ref('1.21.4')
const exportInstanceId = ref('')
const exportOverwrite = ref(false)

async function onExport(): Promise<void> {
  const params: {
    name: string
    mcVersion: string
    installInstanceId?: string
    overwrite?: boolean
  } = {
    name: exportName.value.trim(),
    mcVersion: exportMcVersion.value,
  }
  if (exportInstanceId.value) params.installInstanceId = exportInstanceId.value
  if (exportOverwrite.value) params.overwrite = true
  const ok = await visuals.export(visuals.tab, params)
  if (ok) {
    notifications.push('success', t('vs.notif_exported'), `${exportName.value} (${visuals.tab})`, 'visuals')
  } else {
    notifications.push('error', t('vs.notif_export_failed'), `${visuals.errorCode}: ${visuals.errorMessage}`, 'visuals')
  }
}

// ---- HUD layout ----
function toggleWidget(id: string): void {
  if (!visuals.layout) return
  const found = visuals.layout.find((w) => w.id === id)
  if (found) found.visible = !found.visible
}

function addWidget(id: string): void {
  if (!visuals.layout || visuals.layout.some((w) => w.id === id)) return
  const def = visuals.hud?.defaultLayout.find((w) => w.id === id)
  visuals.layout.push(def ?? { id, x: 4, y: 4, scale: 1, visible: true })
}

async function onSaveHud(): Promise<void> {
  if (!visuals.hudProjectId.trim()) {
    notifications.push('warning', t('vs.notif_project_required'), t('vs.notif_project_hint'), 'visuals')
    return
  }
  const ok = await visuals.persistHudLayout(visuals.hudProjectId.trim())
  if (ok) notifications.push('success', t('vs.notif_hud_saved'), visuals.hudProjectId, 'visuals')
  else notifications.push('error', t('vs.notif_save_failed'), `${visuals.errorCode}: ${visuals.errorMessage}`, 'visuals')
}

function formatSliderLabel(key: string): string {
  return key.replace(/([A-Z])/g, ' $1').toLowerCase()
}

// ---------------------------------------------------------------------------
// Totem 3D viewport (§129–§131)
// ---------------------------------------------------------------------------

const viewportCanvas = ref<HTMLCanvasElement | null>(null)
const viewportWrap = ref<HTMLDivElement | null>(null)
let engine: VoxelEngine | null = null
let resizeObserver: ResizeObserver | null = null

const quality = ref<RenderQuality>('medium')
const debugWireframe = ref(false)
const debugBounds = ref(false)
const debugAxis = ref(false)
const staticFallback = ref<string | null>(null)
const totemSpecJson = ref('')

/** Chỉ mount WebGL khi tab 3D đang mở — rời tab phải dispose (§131). */
watch(() => visuals.tab, (tab) => {
  if (tab === 'totem3d') void mountViewport()
  else void unmountViewport()
})

async function mountViewport(): Promise<void> {
  await nextTick()
  const canvas = viewportCanvas.value
  const wrap = viewportWrap.value
  if (!canvas || !wrap || engine) return

  if (!visuals.totemModel) await visuals.loadTotemModel()
  if (!totemSpecJson.value && visuals.totemModel?.spec) {
    totemSpecJson.value = JSON.stringify(visuals.totemModel.spec, null, 2)
  }

  try {
    engine = await VoxelEngine.create({ canvas, quality: quality.value })
  } catch {
    // Không có WebGL → static renderer fallback (§130)
    await renderStaticFallback()
    return
  }

  applyDocument()
  resizeToWrap()
  engine.render()
  engine.startLoop()
  resizeObserver = new ResizeObserver(() => resizeToWrap())
  resizeObserver.observe(wrap)
}

async function unmountViewport(): Promise<void> {
  // §131: pause loop + remove observers + dispose GPU resources
  resizeObserver?.disconnect()
  resizeObserver = null
  engine?.dispose()
  engine = null
}

onBeforeUnmount(() => {
  void unmountViewport()
  if (rafId) cancelAnimationFrame(rafId)
})

function resizeToWrap(): void {
  const wrap = viewportWrap.value
  if (!wrap) return
  engine?.resize(wrap.clientWidth, Math.max(240, wrap.clientHeight))
}

function applyDocument(): void {
  if (!engine) return
  let spec: unknown = null
  try {
    spec = JSON.parse(totemSpecJson.value || '{}')
  } catch {
    spec = null
  }
  engine.setDocument(specToDocument(spec), {
    wireframe: debugWireframe.value,
    bounds: debugBounds.value,
    axis: debugAxis.value,
  })
}

function onSpecJsonChange(): void {
  applyDocument()
  engine?.render()
}

function onQualityChange(): void {
  engine?.setQuality(quality.value)
}

function onQualityChangeValue(v: string): void {
  quality.value = v as RenderQuality
  onQualityChange()
}

function onDebugToggle(): void {
  applyDocument()
  engine?.render()
}

async function renderStaticFallback(): Promise<void> {
  try {
    const spec = JSON.parse(totemSpecJson.value || 'null')
    if (spec && typeof spec === 'object') {
      staticFallback.value = await renderTotemModel(spec, 256)
    }
  } catch {
    staticFallback.value = null
  }
}

async function onSaveTotemModel(): Promise<void> {
  let spec: Record<string, unknown>
  try {
    spec = JSON.parse(totemSpecJson.value || '{}') as Record<string, unknown>
  } catch {
    notifications.push('error', t('vs.notif_json_error'), t('vs.notif_json_hint'), 'visuals')
    return
  }
  const ok = await visuals.persistTotemModel(spec)
  if (ok) notifications.push('success', t('vs.notif_draft_saved'), '', 'visuals')
  else notifications.push('error', t('vs.notif_save_failed'), `${visuals.errorCode}: ${visuals.errorMessage}`, 'visuals')
}
</script>

<template>
  <section class="vs">
    <header class="vs__header">
      <h1 class="vs__title">
        {{ t('vs.title') }}
      </h1>
      <div
        class="vs__tabs"
        role="tablist"
      >
        <button
          v-for="tab in TABS"
          :key="tab.id"
          class="vs__tab"
          :class="{ 'vs__tab--active': visuals.tab === tab.id }"
          type="button"
          role="tab"
          :aria-selected="visuals.tab === tab.id"
          @click="visuals.setTab(tab.id)"
        >
          {{ tab.label() }}
        </button>
      </div>
    </header>

    <!-- Loading §158 -->
    <div
      v-if="visuals.phase === 'loading'"
      class="vs__state"
    >
      <div
        v-for="n in 4"
        :key="n"
        class="skeleton"
      />
    </div>

    <!-- Offline §68 -->
    <div
      v-else-if="visuals.phase === 'offline'"
      class="vs__state vs__state--warn"
    >
      <p>{{ t('vs.offline') }}</p>
      <button
        class="vs-btn"
        type="button"
        @click="visuals.load()"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <!-- Error §160 -->
    <div
      v-else-if="visuals.phase === 'error'"
      class="vs__state vs__state--error"
    >
      <p>{{ t('vs.error') }}</p>
      <p class="vs__hint">
        {{ visuals.errorCode }}: {{ visuals.errorMessage }}
      </p>
      <button
        class="vs-btn"
        type="button"
        @click="visuals.load()"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <div
      v-else
      class="vs__workspace"
    >
      <!-- Cột 1: presets / controls -->
      <div class="vs-col">
        <h2 class="vs__subtitle">
          {{ visuals.tab === 'hud' ? t('vs.widgets') : t('vs.presets') }}
        </h2>

        <!-- Presets cho crosshair/totem -->
        <template v-if="visuals.tab === 'crosshair' || visuals.tab === 'totem'">
          <div class="vs__preset-grid">
            <button
              v-for="preset in presetsForTab"
              :key="preset.id"
              class="vs-btn vs-btn--chip"
              type="button"
              @click="visuals.selectPreset(preset.id)"
            >
              {{ preset.id }}
            </button>
          </div>
          <div
            v-for="[key] in currentSpecEntries"
            :key="key"
            class="vs__field"
          >
            <span class="vs__field-label">{{ formatSliderLabel(key) }}</span>
            <template v-if="typeof specBool(key) === 'boolean' && key !== 'shape'">
              <input
                type="checkbox"
                :checked="specBool(key)"
                @change="onPatch(key, $event)"
              >
            </template>
            <template v-else-if="key === 'shape'">
              <AppSelect
                :model-value="specString(key)"
                :options="shapeOptions"
                :aria-label="`Shape: ${key}`"
                @update:model-value="(v: string) => onShapeChange(key, v)"
              />
            </template>
            <template v-else-if="key.startsWith('color') || key === 'base' || key === 'accent' || key === 'eye' || key === 'outlineColor'">
              <input
                type="color"
                class="vs__color"
                :value="specString(key) || '#ff4655'"
                @input="onPatch(key, $event)"
              >
            </template>
            <template v-else>
              <input
                type="range"
                min="0"
                :max="key === 'opacity' ? 1 : 16"
                :step="key === 'opacity' ? 0.05 : 1"
                :value="specNumber(key)"
                @input="onPatch(key, $event)"
              >
              <code class="vs__value">{{ specNumber(key) }}</code>
            </template>
          </div>
        </template>

        <!-- HUD: widget list toggle -->
        <template v-else-if="visuals.tab === 'hud' && visuals.hud">
          <div class="vs__preset-grid">
            <button
              v-for="id in visuals.hud.widgets"
              :key="id"
              class="vs-btn vs-btn--chip"
              type="button"
              :class="{ 'vs-btn--chip-on': visuals.layout?.some((w) => w.id === id) }"
              @click="visuals.layout?.some((w) => w.id === id) ? toggleWidget(id) : addWidget(id)"
            >
              {{ id }}
            </button>
          </div>
          <div
            v-for="w in visuals.layout"
            :key="w.id"
            class="vs__field"
          >
            <span class="vs__field-label">{{ w.id }} ({{ w.x }},{{ w.y }})</span>
            <input
              type="range"
              min="0"
              max="800"
              :value="w.x"
              @input="w.x = Number(($event.target as HTMLInputElement).value)"
            >
            <input
              type="range"
              min="0"
              max="450"
              :value="w.y"
              @input="w.y = Number(($event.target as HTMLInputElement).value)"
            >
          </div>
          <div class="vs__field">
            <span class="vs__field-label">{{ t('vs.project_id') }}</span>
            <input
              v-model="visuals.hudProjectId"
              class="vs__input"
              type="text"
              :placeholder="t('vs.project_id_ph')"
            >
            <button
              class="vs-btn"
              type="button"
              @click="onSaveHud"
            >
              {{ t('vs.save_layout') }}
            </button>
          </div>
        </template>

        <!-- Totem 3D: spec JSON + quality + debug modes (§130) -->
        <template v-else-if="visuals.tab === 'totem3d'">
          <div class="vs__field">
            <span class="vs__field-label">{{ t('vs.voxelspec') }}</span>
            <textarea
              v-model="totemSpecJson"
              class="vs__input vs__textarea"
              rows="12"
              spellcheck="false"
              @change="onSpecJsonChange"
            />
          </div>
          <div class="vs__field">
            <span class="vs__field-label">{{ t('vs.render_quality') }}</span>
            <AppSelect
              :model-value="quality"
              :options="qualityOptions"
              :aria-label="t('vs.quality_aria')"
              @update:model-value="onQualityChangeValue"
            />
          </div>
          <label class="vs__field vs__field--row">
            <input
              v-model="debugWireframe"
              type="checkbox"
              @change="onDebugToggle"
            >
            <span>wireframe</span>
          </label>
          <label class="vs__field vs__field--row">
            <input
              v-model="debugBounds"
              type="checkbox"
              @change="onDebugToggle"
            >
            <span>bounds</span>
          </label>
          <label class="vs__field vs__field--row">
            <input
              v-model="debugAxis"
              type="checkbox"
              @change="onDebugToggle"
            >
            <span>axis</span>
          </label>
          <button
            class="vs-btn vs-btn--primary"
            type="button"
            :disabled="visuals.totemModelSaving"
            @click="onSaveTotemModel"
          >
            {{ visuals.totemModelSaving ? t('vs.saving') : t('vs.save_draft') }}
          </button>
        </template>

        <!-- FX -->
        <template v-else-if="visuals.tab === 'fx' && visuals.fx">
          <div class="vs__field">
            <span class="vs__field-label">{{ t('vs.hit_kind') }}</span>
            <AppSelect
              :model-value="String(visuals.hitSpec?.kind ?? 'flash')"
              :options="hitKindOptions"
              :aria-label="t('vs.hit_kind')"
              @update:model-value="onHitKindChange"
            />
          </div>
          <div class="vs__field">
            <span class="vs__field-label">{{ t('vs.hit_color') }}</span>
            <input
              type="color"
              class="vs__color"
              :value="String(visuals.hitSpec?.color ?? '#ff3b30')"
              @input="onPatch('color', $event)"
            >
          </div>
          <div class="vs__field">
            <span class="vs__field-label">{{ t('vs.hit_alpha') }}</span>
            <input
              type="range"
              min="10"
              max="200"
              :value="Number(visuals.hitSpec?.alpha ?? 120)"
              @input="onPatch('alpha', $event)"
            >
          </div>
          <div class="vs__field">
            <span class="vs__field-label">{{ t('vs.particle_shape') }}</span>
            <AppSelect
              :model-value="String(visuals.particleSpec?.shape ?? 'orb')"
              :options="particleShapeOptions"
              :aria-label="t('vs.particle_shape')"
              @update:model-value="onParticleShapeChange"
            />
          </div>
          <button
            class="vs-btn"
            type="button"
            @click="visuals.refreshParticlePreview()"
          >
            {{ t('vs.render_particle') }}
          </button>
        </template>
      </div>

      <!-- Cột 2: preview / 3D viewport -->
      <div class="vs-col vs-col--preview">
        <h2 class="vs__subtitle">
          {{ visuals.tab === 'totem3d' ? t('vs.viewport') : t('vs.preview') }}
        </h2>
        <!-- §131 — canvas chỉ tồn tại khi tab 3D mở -->
        <div
          v-show="visuals.tab === 'totem3d'"
          ref="viewportWrap"
          class="vs__viewport"
        >
          <canvas
            ref="viewportCanvas"
            class="vs__viewport-canvas"
          />
        </div>
        <template v-if="visuals.tab === 'totem3d'">
          <div
            v-if="staticFallback"
            class="vs__fallback"
          >
            <span class="vs__hint">{{ t('vs.webgl_fallback') }}</span>
            <img
              class="vs__preview-img vs__preview-img--small"
              :src="staticFallback"
              alt="Totem 3D static fallback"
            >
          </div>
          <div
            v-else-if="visuals.totemModelLoading"
            class="vs__hint"
          >
            {{ t('vs.loading_draft') }}
          </div>
          <div
            v-else-if="visuals.totemModel?.corrupt"
            class="vs__hint"
          >
            {{ t('vs.corrupt') }}
          </div>
          <div
            v-else-if="visuals.totemModel?.findings?.length"
            class="vs__hint"
          >
            {{ t('vs.findings', { n: visuals.totemModel.findings.length }) }}
          </div>
        </template>
        <template v-else>
          <div
            v-if="visuals.previewPending"
            class="vs__hint"
          >
            {{ t('vs.rendering') }}
          </div>
          <img
            v-else-if="visuals.currentPreview"
            class="vs__preview-img"
            :src="visuals.currentPreview"
            alt="Visual preview"
          >
          <div
            v-else
            class="vs__hint"
          >
            {{ t('vs.no_preview') }}
          </div>
          <code class="vs__hint">data URI từ service — canvas pixel-perfect (§134)</code>
        </template>
      </div>

      <!-- Cột 3: export -->
      <div class="vs-col">
        <h2 class="vs__subtitle">
          {{ t('vs.export_pack') }}
        </h2>
        <div class="vs__field">
          <span class="vs__field-label">{{ t('vs.name') }}</span>
          <input
            v-model="exportName"
            class="vs__input"
            type="text"
            :placeholder="t('vs.name_ph')"
          >
        </div>
        <div class="vs__field">
          <span class="vs__field-label">{{ t('vs.mc_version') }}</span>
          <input
            v-model="exportMcVersion"
            class="vs__input"
            type="text"
            placeholder="1.21.4"
          >
        </div>
        <div class="vs__field">
          <span class="vs__field-label">{{ t('vs.install_instance') }}</span>
          <AppSelect
            :model-value="exportInstanceId"
            :options="exportInstanceOptions"
            :aria-label="t('vs.install_aria')"
            @update:model-value="onExportInstanceChange"
          />
        </div>
        <label class="vs__field vs__field--row">
          <input
            v-model="exportOverwrite"
            type="checkbox"
          >
          <span>{{ t('vs.overwrite') }}</span>
        </label>
        <button
          class="vs-btn vs-btn--primary"
          type="button"
          :disabled="visuals.exporting || !exportName.trim() || visuals.tab === 'hud'"
          @click="onExport"
        >
          {{ visuals.exporting ? t('vs.exporting') : t('vs.export_pack') }}
        </button>
        <p
          v-if="visuals.tab === 'hud'"
          class="vs__hint"
        >
          {{ t('vs.hud_export_hint') }}
        </p>
        <p class="vs__hint">
          {{ t('vs.pipeline_hint') }}
        </p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.vs {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 1200px;
}
.vs__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.vs__title {
  margin: 0;
  font-size: 22px;
}
.vs__subtitle {
  margin: 0;
  font-size: 14px;
}
.vs__tabs {
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
.vs__tab {
  background: transparent;
  border: none;
  color: var(--text-2);
  border-radius: var(--radius-sm);
  padding: 5px 14px;
  cursor: pointer;
  font-size: 12px;
}
.vs__tab--active {
  background: var(--neu-surface);
  color: var(--accent);
  box-shadow:
    2px 2px 5px var(--neu-dark),
    -2px -2px 5px var(--neu-light);
}
.vs__state {
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
.vs__state--warn { border-color: var(--warning); }
.vs__state--error { border-color: var(--danger); }
.vs__hint {
  color: var(--text-3);
  font-size: 12px;
}
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */
.vs__workspace {
  display: grid;
  grid-template-columns: 1.1fr 0.9fr 0.9fr;
  gap: 12px;
  align-items: start;
}
.vs-col {
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
.vs-col--preview {
  align-items: center;
  justify-content: center;
}
.vs__preview-img {
  width: 256px;
  height: 256px;
  object-fit: contain;
  image-rendering: pixelated;
  background:
    repeating-conic-gradient(rgba(0, 0, 0, 0.35) 0% 25%, rgba(255, 255, 255, 0.04) 0% 50%) 0 0 / 16px 16px;
  border-radius: var(--radius-md);
  border: 1px solid var(--glass-border);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 8px 22px var(--neu-dark);
}
.vs__preset-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.vs-btn {
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
.vs-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.vs-btn--primary {
  border-color: var(--accent);
  color: var(--accent);
}
.vs-btn--chip {
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
}
.vs-btn--chip-on {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-soft);
}
.vs__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  color: var(--text-3);
}
.vs__field--row {
  flex-direction: row;
  align-items: center;
  gap: 6px;
}
.vs__field-label {
  font-size: 11px;
  color: var(--text-3);
  text-transform: capitalize;
}
.vs__input {
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  border-radius: 8px;
  color: var(--text-1);
  padding: 7px 10px;
  font-size: 12px;
  width: 100%;
}
.vs__color {
  width: 48px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--border-1);
  border-radius: 6px;
  background: var(--surface-2);
}
.vs__value {
  font-size: 10px;
  color: var(--text-2);
}
.vs__textarea {
  font-family: ui-monospace, monospace;
  font-size: 11px;
  resize: vertical;
}
.vs__viewport {
  width: 100%;
  height: 300px;
  border: 1px solid rgba(0, 0, 0, 0.2);
  border-radius: var(--radius-md);
  overflow: hidden;
  background:
    repeating-conic-gradient(rgba(0, 0, 0, 0.35) 0% 25%, rgba(255, 255, 255, 0.04) 0% 50%) 0 0 / 16px 16px;
  box-shadow:
    inset 3px 3px 10px var(--neu-dark-strong),
    inset -3px -3px 10px var(--neu-light);
}
.vs__viewport-canvas {
  width: 100%;
  height: 100%;
  display: block;
}
.vs__fallback {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: flex-start;
}
.vs__preview-img--small {
  width: 160px;
  height: 160px;
}
</style>
