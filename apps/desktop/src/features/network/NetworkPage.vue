<script setup lang="ts">
import { onMounted } from 'vue'

import { t } from '@/shared/i18n'
import { useNetworkStore } from '@/stores/network.store'

const network = useNetworkStore()

onMounted(() => {
  void network.load()
})

function msClass(ok: boolean): string {
  return ok ? 'net__ms-ok' : 'net__ms-fail'
}

function formatMs(ms: number | null | undefined): string {
  return ms === null || ms === undefined ? '—' : `${ms} ms`
}

/** Màu bar timeline theo RTT tương đối trong probe. */
function barStyle(ms: number | null, maxMs: number): string {
  if (ms === null) return 'height: 100%; opacity: 0.2;'
  const pct = Math.max(8, Math.min(100, (ms / Math.max(1, maxMs)) * 100))
  return `height: ${pct}%;`
}
</script>

<template>
  <section class="net">
    <header class="net__header">
      <h1 class="net__title">
        {{ t('net.title') }}
      </h1>
      <button
        class="net-btn"
        type="button"
        :disabled="network.phase === 'loading'"
        @click="network.load()"
      >
        {{ t('common.reload') }}
      </button>
    </header>

    <!-- Loading §158 -->
    <div
      v-if="network.phase === 'loading'"
      class="net__state"
    >
      <div
        v-for="n in 4"
        :key="n"
        class="skeleton"
      />
    </div>

    <!-- Offline §68 -->
    <div
      v-else-if="network.phase === 'offline'"
      class="net__state net__state--warn"
    >
      <p>{{ t('net.offline') }}</p>
      <button
        class="net-btn"
        type="button"
        @click="network.load()"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <!-- Error §160 -->
    <div
      v-else-if="network.phase === 'error'"
      class="net__state net__state--error"
    >
      <p>{{ t('net.error') }}</p>
      <p class="net__hint">
        {{ network.errorCode }}: {{ network.errorMessage }}
      </p>
      <button
        class="net-btn"
        type="button"
        @click="network.load()"
      >
        {{ t('common.retry') }}
      </button>
    </div>

    <template v-else>
      <!-- Endpoints §42 -->
      <div class="net-card">
        <h2 class="net__subtitle">
          {{ t('net.endpoints') }}
        </h2>
        <div
          v-for="endpoint in network.endpoints"
          :key="endpoint.id"
          class="net__endpoint"
        >
          <span class="net__endpoint-id">{{ endpoint.id }}</span>
          <code class="net__endpoint-host">{{ endpoint.host }}:{{ endpoint.port }}</code>
          <span
            class="net__ms"
            :class="msClass(endpoint.ok)"
          >{{ endpoint.ok ? formatMs(endpoint.ms) : (endpoint.error ?? t('net.fail')) }}</span>
        </div>
      </div>

      <!-- Target form -->
      <div class="net-card">
        <h2 class="net__subtitle">
          {{ t('net.target') }}
        </h2>
        <div class="net__target-row">
          <input
            v-model="network.targetHost"
            class="net__input"
            type="text"
            placeholder="mc.hypixel.net"
          >
          <input
            v-model.number="network.targetPort"
            class="net__input net__input--port"
            type="number"
            min="1"
            max="65535"
          >
        </div>
        <div class="net__actions">
          <button
            class="net-btn net-btn--primary"
            type="button"
            :disabled="network.pinging || !network.targetHost.trim()"
            @click="network.runPing()"
          >
            {{ network.pinging ? t('net.pinging') : t('net.ping_mc') }}
          </button>
          <button
            class="net-btn net-btn--primary"
            type="button"
            :disabled="network.probing || !network.targetHost.trim()"
            @click="network.runProbe(10)"
          >
            {{ network.probing ? t('net.probing') : t('net.probe_rtt') }}
          </button>
          <button
            class="net-btn"
            type="button"
            :disabled="!network.targetHost.trim()"
            @click="network.runTcp()"
          >
            {{ t('net.tcp_check') }}
          </button>
          <button
            class="net-btn"
            type="button"
            :disabled="!network.targetHost.trim()"
            @click="network.runDns()"
          >
            {{ t('net.dns_resolve') }}
          </button>
        </div>

        <!-- TCP single result -->
        <div
          v-if="network.tcp"
          class="net__hint"
        >
          TCP {{ network.tcp.host }}:{{ network.tcp.port }} —
          <span :class="msClass(network.tcp.ok)">
            {{ network.tcp.ok ? formatMs(network.tcp.ms) : network.tcp.error }}
          </span>
        </div>

        <!-- DNS result -->
        <div
          v-if="network.dns"
          class="net__dns"
        >
          <div class="net__hint">
            DNS {{ network.dns.host }} — {{ network.dns.ok ? `${network.dns.addresses.length} ${t('net.addr')} · ${formatMs(network.dns.ms)}` : network.dns.error }}
          </div>
          <code
            v-for="addr in network.dns.addresses"
            :key="addr"
            class="net__addr"
          >
            {{ addr }}
          </code>
        </div>
      </div>

      <!-- MC ping result §42 -->
      <div
        v-if="network.ping"
        class="net-card"
        :class="{ 'net-card--offline': !network.ping.online }"
      >
        <h2 class="net__subtitle">
          {{ t('net.server_ping', { host: network.ping.host, port: network.ping.port }) }}
        </h2>
        <template v-if="network.ping.online">
          <div class="net__row-line">
            <span>MOTD</span>
            <code>{{ network.ping.motd || '—' }}</code>
          </div>
          <div class="net__row-line">
            <span>{{ t('net.players') }}</span>
            <code>{{ network.ping.players?.online ?? 0 }} / {{ network.ping.players?.max ?? 0 }}</code>
          </div>
          <div class="net__row-line">
            <span>{{ t('common.version') }}</span>
            <code>{{ network.ping.version?.name }} ({{ t('net.protocol', { n: network.ping.version?.protocol ?? '?' }) }})</code>
          </div>
          <div class="net__row-line">
            <span>{{ t('net.latency') }}</span>
            <code>{{ formatMs(network.ping.latencyMs) }} ({{ t('net.connect', { ms: formatMs(network.ping.connectMs) }) }})</code>
          </div>
        </template>
        <div
          v-else
          class="net__hint"
        >
          {{ t('net.server_offline', { err: network.ping.error ?? '' }) }}
        </div>
      </div>

      <!-- Probe RTT/jitter + timeline §42 -->
      <div
        v-if="network.probe"
        class="net-card"
      >
        <h2 class="net__subtitle">
          {{ t('net.probe_title', { host: network.probe.host, port: network.probe.port }) }}
        </h2>
        <div class="net__stats">
          <div class="net__stat">
            <span>min</span><code>{{ formatMs(network.probe.stats.min) }}</code>
          </div>
          <div class="net__stat">
            <span>avg</span><code>{{ formatMs(network.probe.stats.avg) }}</code>
          </div>
          <div class="net__stat">
            <span>max</span><code>{{ formatMs(network.probe.stats.max) }}</code>
          </div>
          <div class="net__stat">
            <span>jitter</span><code>{{ formatMs(network.probe.stats.jitter) }}</code>
          </div>
          <div class="net__stat">
            <span>loss</span><code>{{ network.probe.stats.loss }}%</code>
          </div>
        </div>
        <!-- Network timeline: bar mỗi sample, cao = RTT lớn -->
        <div class="net__timeline">
          <div
            v-for="sample in network.probe.timeline"
            :key="sample.seq"
            class="net__bar-wrap"
            :title="`#${sample.seq} ${sample.ok ? formatMs(sample.ms) : sample.error}`"
          >
            <div
              class="net__bar"
              :class="{ 'net__bar--fail': !sample.ok }"
              :style="barStyle(sample.ms, network.probe?.stats.max ?? 1)"
            />
            <span class="net__bar-seq">{{ sample.seq }}</span>
          </div>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.net {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 1200px;
}
.net__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.net__title {
  margin: 0;
  font-size: 22px;
}
.net__subtitle {
  margin: 0;
  font-size: 14px;
}
.net__state {
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
.net__state--warn {
  border-color: rgba(245, 189, 79, 0.4);
}
.net__state--error {
  border-color: rgba(255, 92, 102, 0.45);
}
.net__hint {
  color: var(--text-3);
  font-size: 12px;
}
/* Skeleton loading — toàn cục ở tokens.css (shimmer stagger transform-only) */
.net-card {
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
.net-card--offline {
  border-color: rgba(255, 92, 102, 0.45);
}
.net__endpoint {
  display: grid;
  grid-template-columns: 140px 1fr auto;
  gap: 8px;
  align-items: center;
  font-size: 12px;
}
.net__endpoint-id {
  color: var(--text-2);
}
.net__endpoint-host {
  color: var(--text-3);
  font-size: 11px;
  word-break: break-all;
}
.net__ms {
  font-size: 11px;
}
.net__ms-ok { color: var(--accent); }
.net__ms-fail { color: var(--danger); }
.net__target-row {
  display: flex;
  gap: 8px;
}
.net__input {
  background: var(--neu-surface-inset);
  border: 1px solid rgba(0, 0, 0, 0.2);
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px var(--neu-light);
  border-radius: var(--radius-md);
  color: var(--text-1);
  padding: 8px 12px;
  font-size: 12px;
  flex: 1;
  outline: none;
}
.net__input:focus {
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px var(--neu-light),
    0 0 0 1px var(--accent-glow);
}
.net__input--port {
  flex: 0 0 100px;
}
.net__actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.net-btn {
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
.net-btn:not(:disabled):hover {
  color: var(--text-1);
}
.net-btn:not(:disabled):active {
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.net-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.net-btn--primary {
  border-color: var(--accent);
  color: var(--accent);
}
.net__row-line {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--text-3);
}
.net__row-line code {
  color: var(--text-1);
  font-size: 11px;
  text-align: right;
  word-break: break-all;
}
.net__dns {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.net__addr {
  font-size: 10px;
  color: var(--text-2);
}
.net__stats {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}
.net__stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 10px;
  color: var(--text-3);
  text-transform: uppercase;
}
.net__stat code {
  font-size: 13px;
  color: var(--text-1);
}
.net__timeline {
  display: flex;
  gap: 4px;
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
.net__bar-wrap {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  align-items: center;
  gap: 2px;
  height: 100%;
}
.net__bar {
  width: 100%;
  min-height: 2px;
  background: var(--accent);
  border-radius: 2px 2px 0 0;
}
.net__bar--fail {
  background: var(--danger);
}
.net__bar-seq {
  font-size: 8px;
  color: var(--text-3);
}
</style>
