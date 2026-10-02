<script setup lang="ts">
import { t } from '@/shared/i18n'

/** Stages là hàm để t() đọc locale ref — splash tự dịch khi đổi ngôn ngữ. */
const stages: Array<() => string> = [
  () => t('splash.stage_config'),
  () => t('splash.stage_core'),
  () => t('splash.stage_state'),
  () => t('splash.stage_bus'),
]
</script>

<template>
  <div
    class="splash"
    role="status"
    :aria-label="t('splash.aria')"
  >
    <div class="splash__aurora">
      <div class="aurora aurora--accent" />
      <div class="aurora aurora--info" />
    </div>
    <div class="splash__content">
      <div
        class="splash__mark"
        aria-hidden="true"
      >
        <img
          class="splash__logo"
          src="/antares-logo.png"
          alt=""
          draggable="false"
        >
        <span class="splash__orbit" />
        <span class="splash__orbit splash__orbit--dot" />
      </div>
      <div class="splash__title">
        ANTARES
      </div>
      <div class="splash__stages">
        <span
          v-for="(stage, i) in stages"
          :key="i"
          class="splash__stage"
          :style="{ animationDelay: `${i * 150}ms` }"
        >
          {{ stage() }}
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.splash {
  height: 100vh;
  display: grid;
  place-items: center;
  background: var(--bg-0);
  color: var(--text-1);
  overflow: hidden;
  position: relative;
}
.splash__aurora {
  position: absolute;
  inset: 0;
  pointer-events: none;
}
/* Soft blob gradient — không filter:blur (perf, giống App.vue) */
.aurora {
  position: absolute;
  border-radius: 50%;
  animation: aurora-drift 36s ease-in-out infinite alternate;
}
.aurora--accent {
  width: 720px;
  height: 720px;
  left: -140px;
  top: -200px;
  background: radial-gradient(circle, var(--aurora-accent) 0%, transparent 72%);
}
.aurora--info {
  width: 660px;
  height: 660px;
  right: -180px;
  bottom: -220px;
  background: radial-gradient(circle, var(--aurora-info) 0%, transparent 72%);
  animation-delay: -14s;
}
@keyframes aurora-drift {
  0% {
    transform: translate3d(0, 0, 0) scale(1);
  }
  50% {
    transform: translate3d(40px, -30px, 0) scale(1.05);
  }
  100% {
    transform: translate3d(-32px, 24px, 0) scale(1);
  }
}
.splash__content {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 18px;
}
.splash__mark {
  width: 108px;
  height: 108px;
  display: grid;
  place-items: center;
  position: relative;
}
.splash__logo {
  width: 84px;
  height: 84px;
  border-radius: 22px;
  object-fit: cover;
  box-shadow:
    8px 8px 18px var(--neu-dark-strong),
    -8px -8px 18px var(--neu-light-strong),
    inset 0 1px 0 var(--glass-highlight),
    0 0 42px var(--accent-glow);
  animation: splash-float 2.8s ease-in-out infinite;
}
@keyframes splash-float {
  0%,
  100% {
    transform: translate3d(0, 0, 0) scale(1);
  }
  50% {
    transform: translate3d(0, -6px, 0) scale(1.03);
  }
}
/* Vòng orbit quanh logo — 1 ring + 1 dot chạy quanh */
.splash__orbit {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 1.5px solid rgba(255, 255, 255, 0.08);
  border-top-color: var(--accent);
  animation: splash-spin 1.4s linear infinite;
}
.splash__orbit--dot {
  border: none;
  animation: splash-spin 2.2s linear infinite reverse;
}
.splash__orbit--dot::after {
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
@keyframes splash-spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .splash__logo,
  .splash__orbit {
    animation: none;
  }
}
.splash__title {
  letter-spacing: 0.5em;
  font-weight: 700;
  color: var(--text-2);
}
.splash__stages {
  display: flex;
  gap: 14px;
  font-size: 11px;
  color: var(--text-3);
}
.splash__stage {
  opacity: 0;
  animation: stage-in 300ms ease forwards;
}
@keyframes stage-in {
  to {
    opacity: 1;
  }
}
@media (prefers-reduced-motion: reduce) {
  .splash__stage {
    animation: none;
    opacity: 1;
  }
  .aurora {
    animation: none;
  }
}
</style>
