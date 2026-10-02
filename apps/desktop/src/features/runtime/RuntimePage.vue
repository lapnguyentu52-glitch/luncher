<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import AppSelect from '@/app/components/AppSelect.vue'
import { t } from '@/shared/i18n'
import { useNotificationsStore } from '@/stores/notifications.store'
import { useInstancesStore } from '@/stores/instances.store'
import { useRuntimeStore } from '@/stores/runtime.store'

const runtime = useRuntimeStore()
const instances = useInstancesStore()
const notifications = useNotificationsStore()

const pairInstanceId = ref('')
let pollTimer = 0

/** Options AppSelect: '' = chưa chọn instance để pairing. */
const pairInstanceOptions = computed(() => [
  { value: '', label: t('rt.pair_select') },
  ...instances.items.map((i) => ({ value: i.id, label: i.name })),
])

function onPairInstanceChange(id: string): void {
  pairInstanceId.value = id
}

onMounted(() => {
  void runtime.load()
  void instances.load()
  // §171 — poll metrics cadence 2s (companion đẩy 2–4 packet/s, không spam)
  pollTimer = window.setInterval(() => {
    void runtime.refreshMetrics()
    void runtime.refreshSessions()
  }, 2000)
})

onBeforeUnmount(() => {
  if (pollTimer) window.clearInterval(pollTimer)
})

const fpsPoints = computed(() => runtime.primaryFpsSeries)
const fpsBars = computed(() => {
  const pts = fpsPoints.value.slice(-60)
  const maxFps = Math.max(1, ...pts.map((p) => p.fps ?? 0))
  return pts.map((p) => ({
    height: `${Math.max(4, ((p.fps ?? 0) / maxFps) * 100)}%`,
    title: `${p.fps ?? '?'} fps · ${p.frameMs ?? '?'} ms`,
  }))
})

const selectedPacketId = ref<number | null>(null)

function onSelectPacket(packetId: number): void {
  selectedPacketId.value = selectedPacketId.value === packetId ? null : packetId
}

async function onStart(): Promise<void> {
  const ok = await runtime.start()
  if (ok) notifications.push('success', t('rt.notif_started'), runtime.endpoint?.url ?? '', 'runtime')
  else notifications.push('error', t('rt.notif_start_failed'), `${runtime.errorCode}: ${runtime.errorMessage}`, 'runtime')
}

async function onPair(): Promise<void> {
  if (!pairInstanceId.value) return
  const ok = await runtime.pair(pairInstanceId.value)
  if (ok) {
    notifications.push('success', t('rt.notif_paired'), runtime.pairingFile ?? '', 'runtime')
  } else {
    notifications.push('warning', t('rt.notif_pair_failed'), t('rt.notif_pair_hint'), 'runtime')
  }
}

async function onToggleCapture(event: Event): Promise<void> {
  const enabled = (event.target as HTMLInputElement).checked
  const ok = await runtime.toggleCapture(enabled)
  if (ok) {
    notifications.push('info', enabled ? t('rt.notif_capture_on') : t('rt.notif_capture_off'),
      enabled ? t('rt.notif_capture_hint') : '', 'runtime')
  }
}

async function onExport(): Promise<void> {
  const packets = await runtime.exportPackets()
  if (packets) {
    const blob = new Blob([JSON.stringify(packets, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `antares-packets-${Date.now()}.json`
    a.click()
    URL.revokeObjectURL(url)
    notifications.push('success', t('rt.notif_exported'), t('rt.notif_packets', { n: packets.length }), 'runtime')
  } else {
    notifications.push('error', t('rt.notif_export_failed'), `${runtime.errorCode}: ${runtime.errorMessage}`, 'runtime')
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`
}

function formatTime(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString()
}
</script>

<template>
  <section class="rt">
    <header class="rt__header">
      <h1 class="rt__title">
        {{ t('rt.title') }}
      </h1>
      <button
        class="rt-btn"
        type="button"
        :disabled="runtime.phase === 'loading'"
        @click="runtime.load()"
      >
        {{ t('common.reload') }}
      </button>
    </header>

    <!-- Loading §158 -->
    <div
      v-if="runtime.phase === 'loading'"
      class="rt__state"
    >
      <div
        v-for="n in 4"
        :key="n"
        class="skeleton"
      />
    </div>

    <!-- Offline §68 -->
    <div
      v-else-if="runtime.phase === 'offline'"
      class="rt__state rt__state--warn"
    >
      <p>{{ t('rt.offline') }}</p>
      <button
        class="rt-btn"
        type="button"
        @click="runtime.load()"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <!-- Error §160 -->
    <div
      v-else-if="runtime.phase === 'error'"
      class="rt__state rt__state--error"
    >
      <p>{{ t('rt.error') }}</p>
      <p class="rt__hint">
        {{ runtime.errorCode }}: {{ runtime.errorMessage }}
      </p>
      <button
        class="rt-btn"
        type="button"
        @click="runtime.load()"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <template v-else>
      <!-- Endpoint + pairing §12 -->
      <div class="rt-card">
        <h2 class="rt__subtitle">
          {{ t('rt.endpoint') }}
        </h2>
        <div
          v-if="runtime.endpoint"
          class="rt__row-line"
        >
          <span>URL</span>
          <code>{{ runtime.endpoint.url }}</code>
        </div>
        <div
          v-if="runtime.endpoint"
          class="rt__row-line"
        >
          <span>Token</span>
          <code>{{ runtime.endpoint.token.slice(0, 8) }}…</code>
        </div>
        <div
          v-if="runtime.endpoint"
          class="rt__row-line"
        >
          <span>{{ t('rt.packet_types') }}</span>
          <code>{{ runtime.endpoint.packetTypes.join(', ') }}</code>
        </div>
        <div class="rt__actions">
          <button
            class="rt-btn rt-btn--primary"
            type="button"
            :disabled="runtime.starting"
            @click="onStart"
          >
            {{ runtime.starting ? t('rt.starting') : t('rt.start') }}
          </button>
        </div>
        <div class="rt__pair-row">
          <AppSelect
            :model-value="pairInstanceId"
            :options="pairInstanceOptions"
            :aria-label="t('rt.pair_aria')"
            class="rt__appselect"
            @update:model-value="onPairInstanceChange"
          />
          <button
            class="rt-btn"
            type="button"
            :disabled="!pairInstanceId"
            @click="onPair"
          >
            {{ t('rt.write_companion') }}
          </button>
        </div>
        <p class="rt__hint">
          {{ t('rt.token_hint') }}
        </p>
      </div>

      <!-- Sessions + metrics -->
      <div class="rt__grid">
        <div class="rt-card">
          <h2 class="rt__subtitle">
            {{ t('rt.sessions', { n: runtime.sessions.length }) }}
          </h2>
          <div
            v-if="runtime.sessions.length === 0"
            class="rt__hint"
          >
            {{ t('rt.no_sessions') }}
          </div>
          <div
            v-for="session in runtime.sessions"
            :key="session.sessionId"
            class="rt__row-line"
          >
            <span>{{ session.sessionId }}</span>
            <code>{{ session.instanceId ?? '?' }} · {{ formatTime(session.lastSeen) }}</code>
          </div>
        </div>

        <div class="rt-card">
          <h2 class="rt__subtitle">
            {{ t('rt.fps_timeline') }}
          </h2>
          <div
            v-if="fpsBars.length === 0"
            class="rt__hint"
          >
            {{ t('rt.no_metrics') }}
          </div>
          <div
            v-else
            class="rt__fps-chart"
          >
            <div
              v-for="(bar, i) in fpsBars"
              :key="i"
              class="rt__fps-bar"
              :style="bar"
              :title="bar.title"
            />
          </div>
          <div class="rt__hint">
            {{ t('rt.points', { n: fpsPoints.length }) }}
          </div>
        </div>
      </div>

      <!-- Packet inspector §121/§122 -->
      <div class="rt-card">
        <div class="rt__inspector-head">
          <h2 class="rt__subtitle">
            {{ t('rt.inspector') }}
          </h2>
          <div
            v-if="runtime.packetStats"
            class="rt__hint"
          >
            {{ runtime.packetStats.count }}/{{ runtime.packetStats.cap }} · dropped
            {{ runtime.packetStats.dropped }} · {{ formatBytes(runtime.packetStats.bytes) }}
          </div>
        </div>
        <div class="rt__actions">
          <label class="rt__check">
            <input
              type="checkbox"
              :checked="runtime.packetStats?.capturePayload ?? false"
              @change="onToggleCapture"
            >
            <span>{{ t('rt.capture') }}</span>
          </label>
          <button
            class="rt-btn"
            type="button"
            :disabled="runtime.packetsLoading"
            @click="runtime.refreshPackets()"
          >
            {{ t('rt.refresh') }}
          </button>
          <button
            class="rt-btn"
            type="button"
            @click="runtime.clear()"
          >
            {{ t('rt.clear') }}
          </button>
          <button
            class="rt-btn"
            type="button"
            :disabled="runtime.exporting"
            @click="onExport"
          >
            {{ runtime.exporting ? t('rt.exporting') : t('rt.export') }}
          </button>
        </div>

        <div
          v-if="runtime.packets.length === 0"
          class="rt__hint"
        >
          {{ t('rt.ring_empty') }}
        </div>
        <div
          v-else
          class="rt__packet-list"
        >
          <div
            v-for="packet in runtime.packets"
            :key="packet.packetId"
            class="rt__packet"
            :class="{ 'rt__packet--selected': packet.packetId === selectedPacketId }"
            @click="onSelectPacket(packet.packetId)"
          >
            <span class="rt__packet-id">#{{ packet.packetId }}</span>
            <span class="rt__packet-dir">{{ packet.direction }}</span>
            <code class="rt__packet-name">{{ packet.name }}</code>
            <span class="rt__packet-size">{{ formatBytes(packet.size) }}</span>
            <span class="rt__packet-ts">{{ formatTime(packet.timestamp) }}</span>
            <div
              v-if="packet.packetId === selectedPacketId && packet.payload"
              class="rt__packet-payload"
            >
              <pre>{{ JSON.stringify(packet.payload, null, 2) }}</pre>
            </div>
            <div
              v-else-if="packet.packetId === selectedPacketId"
              class="rt__packet-payload rt__hint"
            >
              {{ t('rt.no_payload') }}
            </div>
          </div>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.rt {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 1200px;
}
.rt__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.rt__title {
  margin: 0;
  font-size: 22px;
}
.rt__subtitle {
  margin: 0;
  font-size: 14px;
}
.rt__state {
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
.rt__state--warn { border-color: rgba(245, 189, 79, 0.4); }
.rt__state--error { border-color: rgba(255, 92, 102, 0.45); }
.rt__hint {
  color: var(--text-3);
  font-size: 12px;
}
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */
.rt-card {
  border: 1px solid rgba(255, 255, 255, 0.04);
  border-radius: var(--radius-lg);
  background: var(--neu-surface);
  box-shadow:
    5px 5px 12px var(--neu-dark),
    -5px -5px 12px var(--neu-light);
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.rt__grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.rt__row-line {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--text-3);
}
.rt__row-line code {
  color: var(--text-1);
  font-size: 11px;
  text-align: right;
  word-break: break-all;
}
.rt__actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
}
.rt__pair-row {
  display: flex;
  gap: 8px;
}
.rt__appselect {
  flex: 1;
}
.rt-btn {
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
.rt-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.rt-btn--primary {
  border-color: var(--accent);
  color: var(--accent);
}
.rt__check {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-2);
}
.rt__fps-chart {
  display: flex;
  gap: 2px;
  align-items: flex-end;
  height: 80px;
  border: 1px solid rgba(0, 0, 0, 0.2);
  border-radius: var(--radius-md);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px var(--neu-light);
  padding: 8px;
}
.rt__fps-bar {
  flex: 1;
  min-height: 2px;
  background: linear-gradient(180deg, var(--accent), rgba(255, 92, 71, 0.55));
  border-radius: 2px 2px 0 0;
  box-shadow: 0 0 6px var(--accent-glow);
}
.rt__inspector-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.rt__packet-list {
  display: flex;
  flex-direction: column;
  gap: 3px;
  max-height: 320px;
  overflow-y: auto;
}
.rt__packet {
  display: grid;
  grid-template-columns: 48px 32px 1fr auto auto;
  gap: 8px;
  align-items: center;
  border: 1px solid rgba(0, 0, 0, 0.18);
  border-radius: var(--radius-sm);
  background: var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light);
  padding: 5px 9px;
  cursor: pointer;
  font-size: 11px;
}
.rt__packet--selected {
  border-color: var(--accent);
}
.rt__packet-id {
  color: var(--text-3);
}
.rt__packet-dir {
  color: var(--info);
  font-size: 10px;
}
.rt__packet-name {
  color: var(--text-1);
  word-break: break-all;
}
.rt__packet-size {
  color: var(--text-3);
  font-size: 10px;
}
.rt__packet-ts {
  color: var(--text-3);
  font-size: 10px;
}
.rt__packet-payload {
  grid-column: 1 / -1;
  border-top: 1px dashed var(--border-1);
  padding-top: 6px;
  margin-top: 4px;
}
.rt__packet-payload pre {
  margin: 0;
  font-size: 10px;
  color: var(--text-2);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 160px;
  overflow-y: auto;
}
</style>
