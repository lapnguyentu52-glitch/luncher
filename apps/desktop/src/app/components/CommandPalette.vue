<script setup lang="ts">
import { computed, ref, watch, nextTick } from 'vue'

import { router } from '@/app/router'
import AboutDialog from '@/app/components/AboutDialog.vue'
import { t } from '@/shared/i18n'
import { useNotificationsStore } from '@/stores/notifications.store'
import { testEvent } from '@/services/appCommands'
import { startLegacy } from '@/services/legacyCommands'

interface Command {
  id: string
  label: () => string
  hint?: string
  run: () => void
}

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()

const query = ref('')
const selectedIndex = ref(0)
const listRef = ref<HTMLElement | null>(null)
const notifications = useNotificationsStore()

const commands = computed<Command[]>(() => [
  {
    id: 'nav.dashboard',
    label: () => t('palette.cmd_dashboard'),
    hint: 'G',
    run: () => void router.push('/'),
  },
  {
    id: 'nav.play',
    label: () => t('palette.cmd_play'),
    hint: 'P',
    run: () => void router.push('/play'),
  },
  {
    id: 'nav.settings',
    label: () => t('palette.cmd_settings'),
    hint: 'S',
    run: () => void router.push('/settings'),
  },
  {
    id: 'dev.test_notification',
    label: () => t('palette.cmd_test_notif'),
    hint: '§36',
    run: () => {
      notifications.push('success', t('palette.cmd_test_notif'), t('palette.notif_test_hint'), 'palette')
    },
  },
  {
    id: 'dev.test_event',
    label: () => t('palette.cmd_test_event'),
    hint: 'IPC',
    run: () => {
      void testEvent()
        .then(() => {
          notifications.push('success', t('palette.notif_event'), t('palette.notif_event_ok'), 'ipc')
        })
        .catch((err: unknown) => {
          const message = err instanceof Error ? err.message : String(err)
          notifications.push('error', t('palette.notif_event'), message, 'ipc')
        })
    },
  },
  {
    id: 'dev.about',
    label: () => t('about.open'),
    hint: '?',
    run: () => {
      aboutOpen.value = true
    },
  },
  {
    id: 'dev.legacy_start',
    label: () => t('palette.cmd_legacy'),
    hint: 'M5',
    run: () => {
      void startLegacy()
        .then((info) => {
          notifications.push(
            'success',
            t('palette.notif_bridge'),
            t('palette.notif_bridge_ok', {
              service: info.service,
              version: info.serviceVersion,
              protocol: info.protocol,
            }),
            'bridge',
          )
        })
        .catch((err: unknown) => {
          const message = err instanceof Error ? err.message : String(err)
          notifications.push('error', t('palette.notif_bridge'), message, 'bridge')
        })
    },
  },
])

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase()
  if (q === '') return commands.value
  return commands.value.filter((c) => c.label().toLowerCase().includes(q))
})

function close(): void {
  emit('update:modelValue', false)
}

function execute(cmd: Command): void {
  close()
  cmd.run()
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    close()
    return
  }
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, filtered.value.length - 1)
    scrollToSelected()
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
    scrollToSelected()
  }
  if (e.key === 'Enter') {
    e.preventDefault()
    const cmd = filtered.value[selectedIndex.value]
    if (cmd !== undefined) execute(cmd)
  }
}

function scrollToSelected(): void {
  void nextTick(() => {
    const el = listRef.value?.children[selectedIndex.value] as HTMLElement | undefined
    el?.scrollIntoView({ block: 'nearest' })
  })
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      query.value = ''
      selectedIndex.value = 0
    }
  },
)

// About dialog — mở từ palette, đóng palette khi mở about
const aboutOpen = ref(false)
function onAboutClosed(): void {
  aboutOpen.value = false
  emit('update:modelValue', false)
}
</script>

<template>
  <Transition name="palette">
    <div
      v-if="modelValue"
      class="palette-overlay"
      @click.self="close"
    >
      <div
        class="palette"
        role="dialog"
        aria-modal="true"
        :aria-label="t('palette.aria')"
        @keydown="onKeydown"
      >
        <div class="palette__search">
          <img
            class="palette__brand"
            src="/antares-logo.png"
            alt=""
            draggable="false"
            aria-hidden="true"
          >
          <input
            v-model="query"
            class="palette__input"
            type="text"
            :placeholder="t('palette.placeholder')"
            autofocus
          >
        </div>
        <ul
          ref="listRef"
          class="palette__list"
        >
          <li
            v-for="(cmd, i) in filtered"
            :key="cmd.id"
            class="palette__item"
            :class="{ 'palette__item--selected': i === selectedIndex }"
            @click="execute(cmd)"
            @mousemove="selectedIndex = i"
          >
            <span>{{ cmd.label() }}</span>
            <span
              v-if="cmd.hint"
              class="palette__hint"
            >{{ cmd.hint }}</span>
          </li>
          <li
            v-if="filtered.length === 0"
            class="palette__empty"
          >
            {{ t('palette.empty') }}
          </li>
        </ul>
        <div class="palette__footer">
          <span><kbd>↑↓</kbd> navigate</span>
          <span><kbd>↵</kbd> run</span>
          <span><kbd>esc</kbd> close</span>
        </div>
      </div>
    </div>
  </Transition>
  <AboutDialog
    v-model="aboutOpen"
    @closed="onAboutClosed"
  />
</template>

<style scoped>
.palette-overlay {
  position: fixed;
  inset: 0;
  background: rgba(4, 5, 9, 0.45);
  backdrop-filter: blur(6px) saturate(120%);
  -webkit-backdrop-filter: blur(6px) saturate(120%);
  display: grid;
  place-items: start center;
  padding-top: 12vh;
  z-index: 100;
}
.palette {
  width: min(560px, 92vw);
  border-radius: var(--radius-lg);
  border: 1px solid var(--glass-border);
  background: var(--glass-bg-strong);
  backdrop-filter: blur(var(--glass-blur-strong)) saturate(170%);
  -webkit-backdrop-filter: blur(var(--glass-blur-strong)) saturate(170%);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 24px 64px var(--neu-dark-strong);
  overflow: hidden;
}
.palette__search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--glass-border);
}
.palette__brand {
  width: 22px;
  height: 22px;
  border-radius: 6px;
  object-fit: cover;
  flex-shrink: 0;
  filter: drop-shadow(0 0 7px var(--accent-glow));
}
.palette__input {
  flex: 1;
  border: none;
  background: transparent;
  color: var(--text-1);
  padding: 6px 0;
  font-size: 14px;
  outline: none;
}
.palette__list {
  list-style: none;
  margin: 0;
  padding: 6px;
  max-height: 320px;
  overflow: auto;
}
.palette__item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 9px 10px;
  border-radius: var(--radius-md);
  font-size: 13px;
  color: var(--text-2);
  cursor: pointer;
  border: 1px solid transparent;
  transition: background var(--motion-instant) ease, box-shadow var(--motion-instant) ease;
}
.palette__item:hover {
  background: var(--neu-surface);
  box-shadow:
    2px 2px 6px var(--neu-dark),
    -2px -2px 6px var(--neu-light);
}
.palette__item--selected {
  background: var(--neu-surface-inset);
  border-color: rgba(0, 0, 0, 0.18);
  color: var(--text-1);
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px var(--neu-light);
}
.palette__hint {
  font-size: 11px;
  color: var(--text-3);
  border: 1px solid var(--glass-border);
  background: rgba(0, 0, 0, 0.25);
  border-radius: 4px;
  padding: 1px 6px;
}
.palette__empty {
  padding: 14px;
  text-align: center;
  color: var(--text-3);
  font-size: 13px;
}
.palette__footer {
  display: flex;
  gap: 14px;
  padding: 8px 14px;
  border-top: 1px solid var(--glass-border);
  font-size: 11px;
  color: var(--text-3);
}
.palette__footer kbd {
  font-family: inherit;
  border: 1px solid var(--glass-border);
  background: rgba(0, 0, 0, 0.25);
  border-radius: 4px;
  padding: 0 5px;
  margin-right: 4px;
}
.palette-enter-active,
.palette-leave-active {
  transition: opacity var(--motion-fast) ease;
}
.palette-enter-active .palette {
  transition: transform var(--motion-normal) ease, opacity var(--motion-normal) ease;
}
.palette-enter-from,
.palette-leave-to {
  opacity: 0;
}
.palette-enter-from .palette {
  transform: translateY(-8px) scale(0.98);
  opacity: 0;
}
</style>
