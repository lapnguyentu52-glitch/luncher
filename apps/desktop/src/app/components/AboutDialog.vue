<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { t } from '@/shared/i18n'
import { getVersion } from '@/services/appCommands'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  closed: []
}>()

const version = ref('--')
const schema = ref('--')

watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      void getVersion()
        .then((info) => {
          version.value = info.app
          schema.value = String(info.schema)
        })
        .catch(() => {
          version.value = '--'
          schema.value = '--'
        })
    }
  },
)

function close(): void {
  emit('update:modelValue', false)
  emit('closed')
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') close()
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <Transition name="about">
    <div
      v-if="modelValue"
      class="about-overlay"
      @click.self="close"
    >
      <div
        class="about"
        role="dialog"
        aria-modal="true"
        :aria-label="t('about.open')"
      >
        <div class="about__hero">
          <img
            class="about__logo"
            src="/antares-logo.png"
            alt=""
            draggable="false"
          >
          <span class="about__orbit" />
          <span class="about__orbit about__orbit--dot" />
        </div>
        <h2 class="about__name">
          ANTARES
        </h2>
        <p class="about__tagline">
          {{ t('about.tagline') }}
        </p>
        <div class="about__meta">
          <div class="about__row">
            <span>{{ t('about.version') }}</span>
            <code>v{{ version }}</code>
          </div>
          <div class="about__row">
            <span>{{ t('about.schema') }}</span>
            <code>{{ schema }}</code>
          </div>
        </div>
        <button
          class="about__close"
          type="button"
          @click="close"
        >
          {{ t('about.close') }}
        </button>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.about-overlay {
  position: fixed;
  inset: 0;
  z-index: 110;
  display: grid;
  place-items: center;
  background: rgba(4, 5, 9, 0.55);
  backdrop-filter: blur(6px) saturate(120%);
  -webkit-backdrop-filter: blur(6px) saturate(120%);
}
.about {
  width: min(360px, 92vw);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 30px 26px 24px;
  border-radius: var(--radius-xl);
  border: 1px solid var(--glass-border);
  /* Frosted solid — không backdrop-filter (perf §5.3) */
  background:
    radial-gradient(ellipse at 50% 0%, var(--accent-soft), transparent 62%),
    var(--glass-bg-solid);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    0 24px 64px var(--neu-dark-strong);
}
.about__hero {
  position: relative;
  width: 108px;
  height: 108px;
  display: grid;
  place-items: center;
  margin-bottom: 6px;
}
.about__logo {
  width: 76px;
  height: 76px;
  border-radius: 20px;
  object-fit: cover;
  box-shadow:
    6px 6px 14px var(--neu-dark-strong),
    -6px -6px 14px var(--neu-light),
    0 0 36px var(--accent-glow);
  animation: about-float 3s ease-in-out infinite;
}
@keyframes about-float {
  0%,
  100% {
    transform: translate3d(0, 0, 0) scale(1);
  }
  50% {
    transform: translate3d(0, -5px, 0) scale(1.02);
  }
}
/* 2 orbit như splash — ring accent + dot ngược chiều */
.about__orbit {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 1.5px solid rgba(255, 255, 255, 0.08);
  border-top-color: var(--accent);
  animation: about-spin 1.6s linear infinite;
}
.about__orbit--dot {
  border: none;
  animation: about-spin 2.4s linear infinite reverse;
}
.about__orbit--dot::after {
  content: '';
  position: absolute;
  top: -3px;
  left: calc(50% - 3px);
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 10px var(--accent-glow);
}
@keyframes about-spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .about__logo,
  .about__orbit {
    animation: none;
  }
}
.about__name {
  margin: 0;
  font-size: 17px;
  font-weight: 800;
  letter-spacing: 0.32em;
  color: var(--text-1);
}
.about__tagline {
  margin: 0 0 8px;
  font-size: 12px;
  color: var(--text-3);
  text-align: center;
}
.about__meta {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 12px;
}
.about__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 7px 12px;
  border-radius: var(--radius-md);
  background: var(--neu-surface-inset);
  border: 1px solid rgba(0, 0, 0, 0.18);
  box-shadow:
    inset 2px 2px 6px var(--neu-dark-strong),
    inset -2px -2px 6px var(--neu-light);
  font-size: 12px;
  color: var(--text-3);
}
.about__row code {
  color: var(--text-1);
  font-size: 12px;
}
.about__close {
  padding: 8px 22px;
  border-radius: 999px;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow:
    3px 3px 8px var(--neu-dark),
    -3px -3px 8px var(--neu-light);
  color: var(--text-2);
  font-size: 12px;
  cursor: pointer;
  transition: color var(--motion-fast) ease, box-shadow var(--motion-instant) ease;
}
.about__close:hover {
  color: var(--text-1);
}
.about__close:active {
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong),
    inset -3px -3px 7px var(--neu-light);
}
/* Transition — transform/opacity only */
.about-enter-active,
.about-leave-active {
  transition: opacity 200ms ease;
}
.about-enter-active .about {
  transition: transform 240ms cubic-bezier(0.34, 1.3, 0.48, 1), opacity 240ms ease;
}
.about-enter-from,
.about-leave-to {
  opacity: 0;
}
.about-enter-from .about {
  transform: translateY(10px) scale(0.96);
  opacity: 0;
}
</style>
