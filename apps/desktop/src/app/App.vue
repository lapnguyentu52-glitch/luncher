<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'

import AppSidebar from '@/app/components/AppSidebar.vue'
import AppTopbar from '@/app/components/AppTopbar.vue'
import AppStatusbar from '@/app/components/AppStatusbar.vue'
import SplashScreen from '@/app/components/SplashScreen.vue'
import CommandPalette from '@/app/components/CommandPalette.vue'
import ToastStack from '@/app/components/ToastStack.vue'
import { useKeyboardShortcuts } from '@/shared/composables/useKeyboardShortcuts'
import { useSessionStore } from '@/stores/session.store'
import { useSettingsStore, applyTheme } from '@/stores/settings.store'
import { initLocale } from '@/shared/i18n'
import { eventIngestion } from '@/services/eventIngestion'
import { EVENT_CHANNEL } from '@/types/protocol'
import { isTauri } from '@/services/ipc'

const booted = ref(false)
const session = useSessionStore()
const settings = useSettingsStore()
const shortcuts = useKeyboardShortcuts()

// Theme từ settings → CSS vars (hydrate + live update); locale VI mặc định
settings.hydrate()
initLocale()
applyTheme(settings.$state)
watch(
  () => [settings.accent, settings.customAccent, settings.background, settings.customBgColor, settings.reducedTransparency],
  () => applyTheme(settings.$state),
)

const paletteOpen = ref(false)
function onPaletteShortcut(): void {
  paletteOpen.value = !paletteOpen.value
}
const paletteShortcut = { key: 'k', ctrl: true, handler: onPaletteShortcut }
shortcuts.register(paletteShortcut)

const splashDone = computed(() => session.bootStage === 'ready' || booted.value)

let timer: ReturnType<typeof setTimeout> | undefined
let unsubEventBridge: (() => void) | undefined

onMounted(() => {
  timer = setTimeout(() => {
    booted.value = true
  }, 1200)

  // Batch 3: nối event bridge Rust → ingestion pipeline khi chạy trong Tauri
  if (isTauri) {
    void import('@tauri-apps/api/event').then(({ listen }) =>
      listen<unknown>(EVENT_CHANNEL, (event) => {
        eventIngestion.ingestRaw(event.payload)
      }),
    ).then((unsub) => {
      unsubEventBridge = unsub
    })
  }
})

onUnmounted(() => {
  if (timer !== undefined) clearTimeout(timer)
  shortcuts.unregister(paletteShortcut)
  unsubEventBridge?.()
})
</script>

<template>
  <!-- Splash → app hand-off mượt (fade, transform/opacity only) -->
  <Transition name="app">
    <SplashScreen v-if="!splashDone" />
    <div
      v-else
      class="app-shell"
    >
      <!-- Ambient aurora + ảnh nền custom (dim overlay giữ chữ đọc được) -->
      <div
        class="app-shell__aurora"
        aria-hidden="true"
      >
        <div class="app-shell__bg-image" />
        <div class="aurora aurora--accent" />
        <div class="aurora aurora--info" />
        <div class="aurora aurora--violet" />
      </div>

      <AppTopbar
        class="app-shell__topbar"
        @palette="onPaletteShortcut"
      />

      <div class="app-shell__body">
        <AppSidebar />
        <main class="app-shell__main">
          <!-- Page transition giữa routes — transform/opacity only (mượt) -->
          <RouterView v-slot="{ Component }">
            <Transition
              name="page"
              mode="out-in"
            >
              <component :is="Component" />
            </Transition>
          </RouterView>
        </main>
      </div>

      <AppStatusbar />
      <CommandPalette v-model="paletteOpen" />
      <ToastStack />
    </div>
  </Transition>
</template>

<style scoped>
.app-shell {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg-0);
  color: var(--text-1);
  overflow: hidden;
}

.app-shell__aurora {
  position: absolute;
  inset: 0;
  z-index: 0;
  pointer-events: none;
  overflow: hidden;
  /* Giam toàn bộ paint/layer aurora trong layer riêng — không lan ra ngoài (perf) */
  contain: strict;
  isolation: isolate;
  transform: translateZ(0);
}
.app-shell__bg-image {
  position: absolute;
  inset: 0;
  background: var(--bg-image, none) center / cover no-repeat;
  opacity: 1;
}
/* Dim overlay đè lên ảnh (không đè aurora) */
.app-shell__bg-image::after {
  content: '';
  position: absolute;
  inset: 0;
  background: rgba(4, 5, 9, var(--bg-dim, 0));
}
/* Soft blob bằng gradient sẵn — KHÔNG filter:blur (rất đắt GPU, gây giật).
   Gradient fade 0% → 72% cho mép mềm tương đương mà render gần như free. */
.aurora {
  position: absolute;
  border-radius: 50%;
  animation: aurora-drift var(--aurora-speed, 36s) ease-in-out infinite alternate;
}
.aurora--accent {
  width: 640px;
  height: 640px;
  left: -160px;
  top: -180px;
  background: radial-gradient(circle, var(--aurora-accent) 0%, transparent 72%);
}
.aurora--info {
  width: 740px;
  height: 740px;
  right: -200px;
  top: 26%;
  background: radial-gradient(circle, var(--aurora-info) 0%, transparent 72%);
  animation-delay: -12s;
}
.aurora--violet {
  width: 700px;
  height: 700px;
  left: 32%;
  bottom: -280px;
  background: radial-gradient(ellipse, var(--aurora-violet) 0%, transparent 72%);
  animation-delay: -24s;
}
@keyframes aurora-drift {
  0% {
    transform: translate3d(0, 0, 0) scale(1);
  }
  50% {
    transform: translate3d(46px, -34px, 0) scale(1.05);
  }
  100% {
    transform: translate3d(-34px, 26px, 0) scale(1);
  }
}
@media (prefers-reduced-motion: reduce) {
  .aurora {
    animation: none;
  }
}

.app-shell__topbar {
  position: relative;
  z-index: 2;
}
.app-shell__body {
  position: relative;
  z-index: 1;
  display: flex;
  flex: 1;
  min-height: 0;
}
.app-shell__main {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 24px;
}
</style>
