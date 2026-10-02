<script setup lang="ts">
import { computed } from 'vue'

import { useNotificationsStore } from '@/stores/notifications.store'

const store = useNotificationsStore()

// Chỉ hiện toast cho 3 notification mới nhất, chưa đọc
const toasts = computed(() => store.items.filter((n) => !n.read).slice(0, 3))

function dismiss(id: string): void {
  store.markRead(id)
}
</script>

<template>
  <div
    class="toasts"
    aria-live="polite"
  >
    <TransitionGroup name="toast">
      <div
        v-for="toast in toasts"
        :key="toast.id"
        class="toast"
        :class="`toast--${toast.severity}`"
        @click="dismiss(toast.id)"
      >
        <span class="toast__title">{{ toast.title }}<span
          v-if="toast.count > 1"
          class="toast__count"
        >×{{ toast.count }}</span></span>
        <span class="toast__message">{{ toast.message }}</span>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  right: 16px;
  bottom: 42px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  z-index: 90;
  width: min(360px, 90vw);
}
.toast {
  padding: 10px 14px;
  border-radius: var(--radius-md);
  border: 1px solid var(--glass-border);
  background: var(--glass-bg-strong);
  backdrop-filter: blur(var(--glass-blur)) saturate(160%);
  -webkit-backdrop-filter: blur(var(--glass-blur)) saturate(160%);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 10px 28px var(--neu-dark);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.toast--info {
  border-left: 3px solid var(--info);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 10px 28px var(--neu-dark),
    0 0 16px rgba(98, 168, 255, 0.12);
}
.toast--success {
  border-left: 3px solid var(--success);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 10px 28px var(--neu-dark),
    0 0 16px rgba(80, 216, 144, 0.12);
}
.toast--warning {
  border-left: 3px solid var(--warning);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 10px 28px var(--neu-dark),
    0 0 16px rgba(245, 189, 79, 0.12);
}
.toast--error {
  border-left: 3px solid var(--danger);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 10px 28px var(--neu-dark),
    0 0 16px rgba(255, 92, 102, 0.14);
}
.toast__title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
}
.toast__count {
  margin-left: 6px;
  color: var(--text-3);
  font-weight: 400;
}
.toast__message {
  font-size: 12px;
  color: var(--text-2);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 160ms ease, transform 160ms ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
@media (prefers-reduced-motion: reduce) {
  .toast-enter-active,
  .toast-leave-active {
    transition: none;
  }
}
</style>
