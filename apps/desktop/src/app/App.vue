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

/** Splash tối đa 1200ms (fallback cũ); tối thiểu 700ms để người dùng kịp thấy animation
 *  thay vì nháy qua khi backend sẵn sàng ngay. Muốn như cũ: đặt MIN_SPLASH_MS = 0. */
const MAX_SPLASH_MS = 1200
const MIN_SPLASH_MS = 700

const booted = ref(false)
const minElapsed = ref(MIN_SPLASH_MS <= 0)
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

const splashDone = computed(
  () => booted.value || (session.bootStage === 'ready' && minElapsed.value),
)

let timer: ReturnType<typeof setTimeout> | undefined
let minTimer: ReturnType<typeof setTimeout> | undefined
let unsubEventBridge: (() => void) | undefined

onMounted(() => {
  timer = setTimeout(() => {
    booted.value = true
  }, MAX_SPLASH_MS)
  if (!minElapsed.value) {
    minTimer = setTimeout(() => {
      minElapsed.value = true
    }, MIN_SPLASH_MS)
  }

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
  if (minTimer !== undefined) clearTimeout(minTimer)
  shortcuts.unregister(paletteShortcut)
  unsubEventBridge?.()
})
</script>

<template>
  <!-- Splash → app hand-off mượt (fade + zoom nhẹ, transform/opacity only) -->
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
        <!-- Vignette + grain tĩnh: chiều sâu "chất liệu", không tốn hiệu năng -->
        <div class="app-shell__vignette" />
        <div class="app-shell__grain" />
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
  height: 100dvh;
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
  will-change: transform;
  animation: aurora-drift var(--aurora-speed, 36s) ease-in-out infinite alternate;
}
.aurora--accent {
  width: 700px;
  height: 700px;
  left: -170px;
  top: -190px;
  background: radial-gradient(circle, var(--aurora-accent) 0%, transparent 72%);
}
.aurora--info {
  width: 780px;
  height: 780px;
  right: -210px;
  top: 24%;
  background: radial-gradient(circle, var(--aurora-info) 0%, transparent 72%);
  animation-delay: -12s;
}
.aurora--violet {
  width: 740px;
  height: 740px;
  left: 30%;
  bottom: -300px;
  background: radial-gradient(ellipse, var(--aurora-violet) 0%, transparent 72%);
  animation-delay: -24s;
}
@keyframes aurora-drift {
  0% {
    transform: translate3d(0, 0, 0) scale(1);
  }
  50% {
    transform: translate3d(48px, -36px, 0) scale(1.06);
  }
  100% {
    transform: translate3d(-36px, 28px, 0) scale(1);
  }
}
@media (prefers-reduced-motion: reduce) {
  .aurora {
    animation: none;
  }
}

/* Vignette: tối nhẹ ở rìa → nội dung ở giữa nổi hơn */
.app-shell__vignette {
  position: absolute;
  inset: 0;
  background: radial-gradient(ellipse at 50% 40%, transparent 58%, rgba(0, 0, 0, 0.3) 100%);
}
/* Grain: 1 lớp nhiễu tĩnh rất mờ */
.app-shell__grain {
  position: absolute;
  inset: 0;
  opacity: 0.03;
  mix-blend-mode: overlay;
  background-image: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
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
  scrollbar-width: thin;
  scrollbar-color: rgba(255, 255, 255, 0.14) transparent;
  scrollbar-gutter: stable;
  overscroll-behavior: contain;
}
.app-shell__main::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}
.app-shell__main::-webkit-scrollbar-track {
  background: transparent;
}
.app-shell__main::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.14);
  border: 2px solid transparent;
  border-radius: 999px;
  background-clip: content-box;
}
.app-shell__main::-webkit-scrollbar-thumb:hover {
  background: var(--accent);
  background-clip: content-box;
}

@media (max-width: 1100px) {
  .app-shell__main {
    padding: 18px;
  }
}
</style>

<!--
  Transition classes phải KHÔNG scoped: Vue áp class lên root của component con,
  scoped style của App.vue không chạm tới. (Nếu dự án đã định nghĩa .app-* / .page-*
  ở CSS global thì xoá khối này đi để tránh trùng.)
-->
<style>
/* Splash → app: splash rời đi bằng fixed để KHÔNG chiếm chỗ cùng lúc với app (tránh giật layout) */
.app-enter-active {
  transition:
    opacity 460ms cubic-bezier(0.22, 1, 0.36, 1),
    transform 560ms cubic-bezier(0.22, 1, 0.36, 1);
}
.app-leave-active {
  position: fixed;
  inset: 0;
  z-index: 1200;
  transition:
    opacity 380ms ease,
    transform 480ms cubic-bezier(0.22, 1, 0.36, 1);
}
.app-enter-from {
  opacity: 0;
  transform: translate3d(0, 8px, 0) scale(0.985);
}
.app-leave-to {
  opacity: 0;
  transform: scale(1.04);
}

/* Chuyển trang: vào trượt lên + hiện dần, ra nhanh để không cảm giác chờ */
.page-enter-active {
  transition:
    opacity 260ms ease-out,
    transform 360ms cubic-bezier(0.22, 1, 0.36, 1);
}
.page-leave-active {
  transition: opacity 110ms ease-in;
}
.page-enter-from {
  opacity: 0;
  transform: translate3d(0, 12px, 0) scale(0.995);
}
.page-leave-to {
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .app-enter-active,
  .app-leave-active,
  .page-enter-active,
  .page-leave-active {
    transition: none;
  }
}
</style>