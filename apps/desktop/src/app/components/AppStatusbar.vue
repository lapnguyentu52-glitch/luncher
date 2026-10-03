<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import { requestCommand, registerBrowserFallback } from '@/services/ipc'
import type { StorageMode } from '@/types/app'

type CoreStatus = 'checking' | 'online' | 'offline'

interface CoreTask {
  id: string
  kind: string
  state: string
  progress: number
}

const coreStatus = ref<CoreStatus>('checking')
const storageMode = ref<StorageMode | null>(null)
const activeTasks = ref(0)
const runningTasks = ref<CoreTask[]>([])
const hubDropped = ref(0)

// Demo mode (browser dev): mock core_status để UI sống không cần backend
registerBrowserFallback('core_status', () => ({
  ok: true,
  data: {
    storageMode: 'portable',
    uptimeMs: 42_000,
    hub: { published: 0, dropped: 0, pendingLatest: 0, pendingCoalesce: 0, pendingBatched: 0, pendingLossless: 0 },
    activeTasks: [] as CoreTask[],
    rustOnly: false,
  },
  warnings: [],
}))

const taskPercent = computed(() => {
  const t = runningTasks.value[0]
  return t ? Math.max(4, Math.min(100, Math.round(t.progress))) : 0
})

/** Chu circumference r=4.5 → 2πr ≈ 28.27; dash = progress phần, phần còn lại track. */
const ringDash = computed(() => `${(taskPercent.value / 100) * 28.27} 28.27`)

let pollTimer = 0
onMounted(async () => {
  const tick = async (): Promise<void> => {
    const res = await requestCommand('core_status')
    if (res.ok && res.data !== undefined) {
      coreStatus.value = 'online'
      storageMode.value = res.data.storageMode
      activeTasks.value = res.data.activeTasks.length
      runningTasks.value = (res.data.activeTasks as CoreTask[]).filter(
        (t) => t.state === 'RUNNING' || t.state === 'QUEUED',
      )
      hubDropped.value = res.data.hub.dropped
    } else {
      coreStatus.value = 'offline'
      runningTasks.value = []
    }
  }
  await tick()
  pollTimer = window.setInterval(() => void tick(), 4000)
})
onBeforeUnmount(() => {
  if (pollTimer) window.clearInterval(pollTimer)
})
</script>

<template>
  <footer class="statusbar">
    <span
      class="statusbar__dot"
      :class="[
        `statusbar__dot--${coreStatus}`,
        { 'statusbar__dot--pulse': coreStatus === 'online' },
      ]"
      aria-hidden="true"
    />
    <span class="statusbar__status">
      Core {{ coreStatus === 'checking' ? '…' : coreStatus }}
    </span>
    <span
      v-if="storageMode"
      class="statusbar__chip"
    >
      <svg
        class="statusbar__chip-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <rect
          x="3"
          y="4"
          width="18"
          height="12"
          rx="2"
        />
        <path d="M8 20h8M12 16v4" />
      </svg>
      {{ storageMode }}
    </span>
    <span
      v-if="runningTasks.length > 0"
      class="statusbar__chip statusbar__chip--task"
      :title="runningTasks.map((t) => `${t.kind} ${Math.round(t.progress)}%`).join(' · ')"
    >
      <span class="statusbar__mini-ring">
        <svg viewBox="0 0 12 12">
          <circle
            cx="6"
            cy="6"
            r="4.5"
            class="statusbar__mini-ring-track"
          />
          <circle
            cx="6"
            cy="6"
            r="4.5"
            class="statusbar__mini-ring-fill"
            :stroke-dasharray="ringDash"
          />
        </svg>
      </span>
      {{ runningTasks.length }} task · {{ taskPercent }}%
    </span>
    <span
      v-else-if="activeTasks > 0"
      class="statusbar__chip"
    >{{ activeTasks }} task(s)</span>
    <span
      v-if="hubDropped > 0"
      class="statusbar__chip statusbar__chip--warn"
    >
      <svg
        class="statusbar__chip-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
      >
        <path d="M12 9v4M12 17h.01M10.3 3.9L1.8 18a2 2 0 001.7 3h17a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z" />
      </svg>
      {{ hubDropped }} dropped
    </span>
    <span class="statusbar__spacer" />
    <span class="statusbar__meta">Antares 4.0</span>
  </footer>
</template>

<style scoped>
.statusbar {
  height: 28px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  background: var(--glass-bg-strong);
  backdrop-filter: blur(var(--glass-blur)) saturate(150%);
  -webkit-backdrop-filter: blur(var(--glass-blur)) saturate(150%);
  border-top: 1px solid var(--glass-border);
  box-shadow: inset 0 1px 0 var(--glass-highlight);
  font-size: 11px;
  color: var(--text-3);
  position: relative;
  z-index: 2;
}
.statusbar__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.statusbar__dot--online {
  background: var(--success);
  box-shadow: 0 0 8px rgba(80, 216, 144, 0.6);
}
.statusbar__dot--pulse {
  animation: sb-dot-pulse 2.4s ease-in-out infinite;
}
@keyframes sb-dot-pulse {
  0%,
  100% {
    box-shadow: 0 0 4px rgba(80, 216, 144, 0.45);
  }
  50% {
    box-shadow: 0 0 12px rgba(80, 216, 144, 0.85);
  }
}
.statusbar__dot--offline {
  background: var(--text-3);
}
.statusbar__dot--checking {
  background: var(--warning);
  box-shadow: 0 0 8px rgba(245, 189, 79, 0.5);
}
.statusbar__chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 1px solid var(--glass-border);
  background: rgba(0, 0, 0, 0.22);
  box-shadow:
    inset 2px 2px 4px var(--neu-dark),
    inset -2px -2px 4px var(--neu-light);
  border-radius: 6px;
  padding: 1px 7px;
  font-size: 10px;
}
.statusbar__chip-icon {
  width: 10px;
  height: 10px;
  opacity: 0.7;
}
.statusbar__chip--task {
  color: var(--accent);
  border-color: rgba(255, 92, 71, 0.35);
  font-variant-numeric: tabular-nums;
}
.statusbar__mini-ring {
  display: inline-flex;
}
.statusbar__mini-ring svg {
  width: 12px;
  height: 12px;
  transform: rotate(-90deg);
}
.statusbar__mini-ring circle {
  fill: none;
  stroke-width: 2.4;
}
.statusbar__mini-ring-track {
  stroke: rgba(255, 255, 255, 0.1);
}
.statusbar__mini-ring-fill {
  stroke: var(--accent);
  stroke-linecap: round;
  filter: drop-shadow(0 0 3px var(--accent-glow));
  transition: stroke-dasharray 400ms ease;
}
.statusbar__chip--warn {
  color: var(--warning);
  border-color: rgba(245, 189, 79, 0.4);
}
.statusbar__spacer {
  flex: 1;
}
@media (prefers-reduced-motion: reduce) {
  .statusbar__dot--pulse {
    animation: none;
  }
}
</style>
