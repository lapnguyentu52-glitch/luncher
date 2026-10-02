<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import { t } from '@/shared/i18n'
import { useNotificationsStore } from '@/stores/notifications.store'
import { requestCommand, registerBrowserFallback } from '@/services/ipc'

// Topbar v3 — brand + command pill + runtime chips + progress ring + bell.
// Chips/ring/bell bind dữ liệu thật: core_status (activeTasks) + notifications.

interface CoreTask {
  id: string
  kind: string
  state: string
  progress: number
  message?: string
}

const notifications = useNotificationsStore()
const router = useRouter()

const coreOnline = ref(true)
const activeTasks = ref<CoreTask[]>([])

// Browser dev fallback — giữ đồng bộ với AppStatusbar
registerBrowserFallback('core_status', () => ({
  ok: true,
  data: {
    storageMode: 'portable',
    uptimeMs: 42_000,
    hub: { published: 0, dropped: 0, pendingLatest: 0, pendingCoalesce: 0, pendingBatched: 0, pendingLossless: 0 },
    activeTasks: [] as CoreTask[],
  },
  warnings: [],
}))

let pollTimer = 0
onMounted(() => {
  void refreshStatus()
  pollTimer = window.setInterval(() => void refreshStatus(), 4000)
})
onBeforeUnmount(() => {
  if (pollTimer) window.clearInterval(pollTimer)
})

async function refreshStatus(): Promise<void> {
  try {
    const res = await requestCommand('core_status')
    if (res.ok && res.data !== undefined) {
      coreOnline.value = true
      activeTasks.value = res.data.activeTasks ?? []
    } else {
      coreOnline.value = false
      activeTasks.value = []
    }
  } catch {
    coreOnline.value = false
  }
}

const runningTask = computed(() =>
  activeTasks.value.find((t) => t.state === 'RUNNING' || t.state === 'QUEUED') ?? null,
)
const taskPercent = computed(() =>
  runningTask.value ? Math.max(4, Math.min(100, Math.round(runningTask.value.progress))) : 0,
)

const emit = defineEmits<{ palette: [] }>()
const isMac = navigator.platform.toLowerCase().includes('mac')
const kbdHint = computed(() => (isMac ? '⌘K' : 'Ctrl+K'))

const bellOpen = ref(false)
function toggleBell(): void {
  bellOpen.value = !bellOpen.value
}
function openNotification(id: string): void {
  notifications.markRead(id)
}
function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

// Live clock — giờ hệ thống trên topbar
const now = ref(new Date())
let clockTimer = 0
onMounted(() => {
  clockTimer = window.setInterval(() => {
    now.value = new Date()
  }, 30_000)
})
onBeforeUnmount(() => {
  if (clockTimer) window.clearInterval(clockTimer)
})
const clock = computed(() =>
  now.value.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
)

function goDashboard(): void {
  void router.push('/')
}
</script>

<template>
  <header class="topbar glass">
    <button
      class="topbar__brand"
      type="button"
      :title="t('topbar.to_dashboard')"
      @click="goDashboard"
    >
      <img
        class="topbar__logo"
        src="/antares-logo.png"
        alt=""
        draggable="false"
      >
      <span class="topbar__brand-text">ANTARES</span>
    </button>

    <span class="topbar__clock">{{ clock }}</span>

    <button
      class="topbar__command"
      type="button"
      @click="emit('palette')"
    >
      <svg
        class="topbar__search-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
      >
        <circle
          cx="11"
          cy="11"
          r="7"
        />
        <path d="M20 20l-3.5-3.5" />
      </svg>
      <span class="topbar__command-label">Search / Command</span>
      <span class="topbar__kbd">{{ kbdHint }}</span>
    </button>

    <!-- Runtime chip — core status -->
    <div
      class="chip"
      :class="{ 'chip--off': !coreOnline }"
      :title="t('topbar.core_status')"
    >
      <span
        class="chip__dot"
        :class="{ 'chip__dot--pulse': coreOnline }"
      />
      <span class="chip__label">Core</span>
    </div>

    <!-- Download progress ring — hiện khi có task chạy -->
    <Transition name="pop">
      <div
        v-if="runningTask"
        class="chip chip--task"
        :title="runningTask.message ?? runningTask.kind"
      >
        <svg
          class="ring"
          viewBox="0 0 20 20"
        >
          <circle
            class="ring__track"
            cx="10"
            cy="10"
            r="8"
          />
          <circle
            class="ring__fill"
            cx="10"
            cy="10"
            r="8"
            :stroke-dasharray="`${(taskPercent / 100) * 50.27} 50.27`"
          />
        </svg>
        <span class="chip__label">{{ taskPercent }}%</span>
      </div>
    </Transition>

    <!-- Bell — badge đếm + dropdown -->
    <div class="bell-wrap">
      <button
        class="bell"
        :class="{ 'bell--live': notifications.unreadCount > 0 }"
        type="button"
        :aria-label="t('topbar.notifications')"
        @click="toggleBell"
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
        >
          <path d="M18 8a6 6 0 10-12 0c0 7-3 8-3 8h18s-3-1-3-8" />
          <path d="M13.7 20a2 2 0 01-3.4 0" />
        </svg>
        <Transition name="pop">
          <span
            v-if="notifications.unreadCount > 0"
            class="bell__badge"
          >{{ notifications.unreadCount > 9 ? '9+' : notifications.unreadCount }}</span>
        </Transition>
      </button>
      <Transition name="drop">
        <div
          v-if="bellOpen"
          class="bell-panel glass"
        >
          <div class="bell-panel__head">
            <span>Notifications</span>
            <span
              v-if="notifications.unreadCount > 0"
              class="bell-panel__count"
            >{{ notifications.unreadCount }} mới</span>
            <span
              v-else
              class="bell-panel__count bell-panel__count--off"
            >đã đọc hết</span>
          </div>
          <div
            v-if="notifications.items.length === 0"
            class="bell-panel__empty"
          >
            Chưa có gì — mọi thứ chạy ngon.
          </div>
          <button
            v-for="n in notifications.items.slice(0, 6)"
            :key="n.id"
            class="bell-panel__item"
            :class="{ 'bell-panel__item--unread': !n.read }"
            type="button"
            @click="openNotification(n.id)"
          >
            <span
              class="bell-panel__sev"
              :class="`bell-panel__sev--${n.severity}`"
            />
            <span class="bell-panel__body">
              <span class="bell-panel__title">{{ n.title }} <em v-if="n.count > 1">×{{ n.count }}</em></span>
              <span class="bell-panel__msg">{{ n.message }}</span>
            </span>
            <span class="bell-panel__time">{{ formatTime(n.timestamp) }}</span>
          </button>
        </div>
      </Transition>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  height: 52px;
  display: flex;
  align-items: center;
  padding: 0 16px;
  gap: 12px;
  background: var(--glass-bg-strong);
  backdrop-filter: blur(var(--glass-blur)) saturate(160%);
  -webkit-backdrop-filter: blur(var(--glass-blur)) saturate(160%);
  border: none;
  border-bottom: 1px solid var(--glass-border);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 8px 24px var(--neu-dark);
}

/* Brand */
.topbar__brand {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  background: none;
  border: none;
  cursor: pointer;
  padding: 4px 6px;
  border-radius: var(--radius-md);
  transition: background var(--motion-fast) ease;
}
.topbar__brand:hover {
  background: rgba(255, 255, 255, 0.04);
}
.topbar__logo {
  width: 24px;
  height: 24px;
  border-radius: 7px;
  object-fit: cover;
  filter: drop-shadow(0 0 8px var(--accent-glow));
}
.topbar__brand-text {
  font-size: 13px;
  font-weight: 800;
  letter-spacing: 0.24em;
  color: var(--text-1);
}

/* Live clock */
.topbar__clock {
  font-size: 11px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
  padding: 2px 8px;
  border: 1px solid var(--glass-border);
  border-radius: 999px;
  background: rgba(0, 0, 0, 0.2);
}

/* Command pill */
.topbar__command {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  /* Trên nền topbar đã blur — không cần backdrop-filter riêng (perf) */
  background: var(--glass-bg-solid);
  border: 1px solid var(--glass-border);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light),
    inset 0 1px 0 var(--glass-highlight);
  color: var(--text-3);
  border-radius: 999px;
  padding: 6px 14px;
  cursor: pointer;
  font-size: 12px;
  transition: color var(--motion-fast) ease, box-shadow var(--motion-fast) ease, border-color var(--motion-fast) ease;
}
.topbar__command:hover {
  color: var(--text-1);
  border-color: rgba(255, 92, 71, 0.35);
  box-shadow:
    4px 4px 10px var(--neu-dark),
    -4px -4px 10px var(--neu-light),
    inset 0 1px 0 var(--glass-highlight),
    0 0 14px var(--accent-glow);
}
.topbar__command:active {
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.topbar__search-icon {
  width: 14px;
  height: 14px;
}
.topbar__kbd {
  font-size: 10px;
  color: var(--text-3);
  border: 1px solid var(--glass-border);
  border-radius: 4px;
  padding: 1px 6px;
  background: rgba(0, 0, 0, 0.25);
}

/* Runtime chip */
.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border: 1px solid var(--glass-border);
  background: rgba(0, 0, 0, 0.22);
  box-shadow:
    inset 2px 2px 4px var(--neu-dark),
    inset -2px -2px 4px var(--neu-light);
  border-radius: 999px;
  font-size: 11px;
  color: var(--text-2);
}
.chip--off {
  color: var(--text-3);
}
.chip__dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--success);
  box-shadow: 0 0 8px rgba(80, 216, 144, 0.7);
}
.chip--off .chip__dot {
  background: var(--text-3);
  box-shadow: none;
}
.chip__dot--pulse {
  animation: dot-pulse 2.4s ease-in-out infinite;
}
@keyframes dot-pulse {
  0%, 100% {
    box-shadow: 0 0 4px rgba(80, 216, 144, 0.5);
  }
  50% {
    box-shadow: 0 0 12px rgba(80, 216, 144, 0.9);
  }
}
.chip__label {
  font-variant-numeric: tabular-nums;
}

/* Progress ring chip */
.chip--task {
  gap: 7px;
  border-color: rgba(255, 92, 71, 0.35);
}
.ring {
  width: 18px;
  height: 18px;
  transform: rotate(-90deg);
}
.ring circle {
  fill: none;
  stroke-width: 3;
}
.ring__track {
  stroke: rgba(255, 255, 255, 0.08);
}
.ring__fill {
  stroke: var(--accent);
  stroke-linecap: round;
  filter: drop-shadow(0 0 4px var(--accent-glow));
  transition: stroke-dasharray 400ms ease;
}

/* Bell */
.bell-wrap {
  position: relative;
}
.bell {
  position: relative;
  display: grid;
  place-items: center;
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 14, 0.04);
  color: var(--text-2);
  cursor: pointer;
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  transition: color var(--motion-fast) ease, box-shadow var(--motion-instant) ease;
}
.bell:hover {
  color: var(--text-1);
}
.bell:active {
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
.bell svg {
  width: 17px;
  height: 17px;
}
.bell--live svg {
  animation: bell-swing 2.6s ease-in-out infinite;
}
@keyframes bell-swing {
  0%, 84%, 100% {
    transform: rotate(0);
  }
  88% {
    transform: rotate(11deg);
  }
  92% {
    transform: rotate(-9deg);
  }
  96% {
    transform: rotate(5deg);
  }
}
.bell__badge {
  position: absolute;
  top: -3px;
  right: -3px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  display: grid;
  place-items: center;
  font-size: 9px;
  font-weight: 800;
  color: #fff;
  background: var(--danger);
  border-radius: 999px;
  box-shadow: 0 0 10px rgba(255, 92, 102, 0.55);
}
.bell-panel {
  position: absolute;
  right: 0;
  top: calc(100% + 10px);
  width: min(340px, 88vw);
  border-radius: var(--radius-lg);
  background: var(--glass-bg-strong);
  backdrop-filter: blur(var(--glass-blur-strong)) saturate(160%);
  -webkit-backdrop-filter: blur(var(--glass-blur-strong)) saturate(160%);
  border: 1px solid var(--glass-border);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 24px 56px var(--neu-dark-strong);
  overflow: hidden;
  z-index: 50;
}
.bell-panel__head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  font-size: 12px;
  font-weight: 700;
  color: var(--text-1);
  border-bottom: 1px solid var(--glass-border);
}
.bell-panel__count {
  margin-left: auto;
  font-size: 10px;
  font-weight: 600;
  color: var(--accent);
}
.bell-panel__count--off {
  color: var(--text-3);
  font-weight: 400;
}
.bell-panel__empty {
  padding: 18px;
  text-align: center;
  font-size: 12px;
  color: var(--text-3);
}
.bell-panel__item {
  width: 100%;
  display: flex;
  gap: 10px;
  align-items: flex-start;
  padding: 9px 12px;
  background: none;
  border: none;
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  cursor: pointer;
  text-align: left;
  transition: background var(--motion-instant) ease;
}
.bell-panel__item:hover {
  background: rgba(255, 255, 255, 0.03);
}
.bell-panel__item--unread {
  background: rgba(255, 92, 71, 0.05);
}
.bell-panel__item:last-child {
  border-bottom: none;
}
.bell-panel__sev {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  margin-top: 4px;
  flex-shrink: 0;
}
.bell-panel__sev--info {
  background: var(--info);
}
.bell-panel__sev--success {
  background: var(--success);
}
.bell-panel__sev--warning {
  background: var(--warning);
}
.bell-panel__sev--error {
  background: var(--danger);
}
.bell-panel__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.bell-panel__title {
  font-size: 12px;
  color: var(--text-1);
  font-weight: 600;
}
.bell-panel__title em {
  color: var(--text-3);
  font-style: normal;
  font-weight: 400;
}
.bell-panel__msg {
  font-size: 11px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bell-panel__time {
  font-size: 10px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}

/* Transitions */
.pop-enter-active,
.pop-leave-active {
  transition: opacity var(--motion-fast) ease, transform var(--motion-fast) cubic-bezier(0.34, 1.56, 0.64, 1);
}
.pop-enter-from,
.pop-leave-to {
  opacity: 0;
  transform: scale(0.6);
}
.drop-enter-active,
.drop-leave-active {
  transition: opacity var(--motion-fast) ease, transform var(--motion-normal) cubic-bezier(0.34, 1.56, 0.64, 1);
}
.drop-enter-from,
.drop-leave-to {
  opacity: 0;
  transform: translateY(-6px) scale(0.98);
}

@media (prefers-reduced-motion: reduce) {
  .chip__dot--pulse,
  .bell--live svg {
    animation: none;
  }
}
</style>
