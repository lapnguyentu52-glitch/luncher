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
    <div
      class="splash__aurora"
      aria-hidden="true"
    >
      <div class="aurora aurora--accent" />
      <div class="aurora aurora--info" />
    </div>
    <!-- Lưới chấm mờ dần ra rìa + vignette: thêm chiều sâu, hoàn toàn tĩnh -->
    <div
      class="splash__grid"
      aria-hidden="true"
    />
    <div
      class="splash__vignette"
      aria-hidden="true"
    />

    <div class="splash__content">
      <div
        class="splash__mark"
        aria-hidden="true"
      >
        <span class="splash__pulse" />
        <span class="splash__pulse splash__pulse--late" />
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

      <!-- Thanh tiến trình không xác định — chỉ transform -->
      <div
        class="splash__bar"
        aria-hidden="true"
      >
        <i />
      </div>

      <div class="splash__stages">
        <span
          v-for="(stage, i) in stages"
          :key="i"
          class="splash__stage"
          :style="{ '--i': i }"
        >
          <i class="splash__dot" />
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
  will-change: transform;
  animation: aurora-drift 36s ease-in-out infinite alternate;
}
.aurora--accent {
  width: 760px;
  height: 760px;
  left: -160px;
  top: -220px;
  background: radial-gradient(circle, var(--aurora-accent) 0%, transparent 72%);
}
.aurora--info {
  width: 700px;
  height: 700px;
  right: -200px;
  bottom: -240px;
  background: radial-gradient(circle, var(--aurora-info) 0%, transparent 72%);
  animation-delay: -14s;
}
@keyframes aurora-drift {
  0% {
    transform: translate3d(0, 0, 0) scale(1);
  }
  50% {
    transform: translate3d(44px, -30px, 0) scale(1.06);
  }
  100% {
    transform: translate3d(-34px, 26px, 0) scale(1);
  }
}

/* Lưới chấm — mờ dần về rìa nhờ mask, không animate */
.splash__grid {
  position: absolute;
  inset: 0;
  pointer-events: none;
  opacity: 0.5;
  background: radial-gradient(rgba(255, 255, 255, 0.09) 1px, transparent 1.4px) 0 0 / 26px 26px;
  -webkit-mask-image: radial-gradient(520px 360px at 50% 48%, #000 0%, transparent 100%);
  mask-image: radial-gradient(520px 360px at 50% 48%, #000 0%, transparent 100%);
}
.splash__vignette {
  position: absolute;
  inset: 0;
  pointer-events: none;
  background: radial-gradient(ellipse at 50% 50%, transparent 55%, rgba(0, 0, 0, 0.38) 100%);
}

.splash__content {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 20px;
  animation: splash-rise 700ms cubic-bezier(0.22, 1, 0.36, 1) both;
}
@keyframes splash-rise {
  from {
    opacity: 0;
    transform: translate3d(0, 14px, 0) scale(0.97);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.splash__mark {
  width: 124px;
  height: 124px;
  display: grid;
  place-items: center;
  position: relative;
}
.splash__logo {
  width: 88px;
  height: 88px;
  border-radius: 24px;
  object-fit: cover;
  box-shadow:
    8px 8px 18px var(--neu-dark-strong),
    -8px -8px 18px var(--neu-light-strong),
    inset 0 1px 0 var(--glass-highlight),
    0 0 52px var(--accent-glow);
  animation: splash-float 2.8s ease-in-out infinite;
}
@keyframes splash-float {
  0%,
  100% {
    transform: translate3d(0, 0, 0) scale(1);
  }
  50% {
    transform: translate3d(0, -6px, 0) scale(1.035);
  }
}

/* Sóng toả ra từ logo — chỉ transform + opacity */
.splash__pulse {
  position: absolute;
  inset: 18px;
  border-radius: 28px;
  border: 1.5px solid var(--accent);
  opacity: 0;
  animation: splash-ping 2.6s cubic-bezier(0, 0, 0.2, 1) infinite;
}
.splash__pulse--late {
  animation-delay: 1.3s;
}
@keyframes splash-ping {
  0% {
    transform: scale(0.9);
    opacity: 0.5;
  }
  80%,
  100% {
    transform: scale(1.55);
    opacity: 0;
  }
}

/* Vòng orbit kiểu sao chổi: đuôi gradient mờ dần + 1 chấm chạy ngược chiều */
.splash__orbit {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  background: conic-gradient(from 0deg, transparent 0 55%, var(--accent) 100%);
  -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 1.5px));
  mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 1.5px));
  animation: splash-spin 1.4s linear infinite;
}
.splash__orbit--dot {
  inset: 8px;
  background: none;
  -webkit-mask: none;
  mask: none;
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
  box-shadow: 0 0 12px var(--accent-glow);
}
@keyframes splash-spin {
  to {
    transform: rotate(360deg);
  }
}

.splash__title {
  letter-spacing: 0.55em;
  padding-left: 0.55em; /* bù letter-spacing để chữ nằm đúng giữa */
  font-size: 15px;
  font-weight: 700;
  color: var(--text-2);
  background: linear-gradient(100deg, var(--text-2) 30%, var(--accent) 120%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}

/* Thanh tải mảnh */
.splash__bar {
  width: 168px;
  height: 3px;
  border-radius: 3px;
  overflow: hidden;
  background: rgba(255, 255, 255, 0.07);
}
.splash__bar > i {
  display: block;
  width: 38%;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, transparent, var(--accent), transparent);
  box-shadow: 0 0 10px var(--accent-glow);
  animation: splash-bar 1.3s cubic-bezier(0.55, 0.06, 0.32, 1) infinite;
}
@keyframes splash-bar {
  0% {
    transform: translateX(-110%);
  }
  100% {
    transform: translateX(320%);
  }
}

/* Các bước khởi động: viên nhỏ + chấm, hiện lần lượt */
.splash__stages {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
  max-width: 560px;
  font-size: 11px;
  color: var(--text-3);
}
.splash__stage {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 4px 11px;
  border-radius: 999px;
  border: 1px solid rgba(255, 255, 255, 0.06);
  background: rgba(255, 255, 255, 0.025);
  opacity: 0;
  animation: stage-in 380ms cubic-bezier(0.22, 1, 0.36, 1) forwards;
  animation-delay: calc(var(--i) * 170ms + 250ms);
}
@keyframes stage-in {
  from {
    opacity: 0;
    transform: translate3d(0, 6px, 0);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
.splash__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 8px var(--accent-glow);
  animation: dot-beat 1.6s ease-in-out infinite;
  animation-delay: calc(var(--i) * 170ms + 600ms);
}
@keyframes dot-beat {
  0%,
  100% {
    transform: scale(1);
    opacity: 1;
  }
  50% {
    transform: scale(0.55);
    opacity: 0.55;
  }
}

@media (prefers-reduced-motion: reduce) {
  .aurora,
  .splash__content,
  .splash__logo,
  .splash__orbit,
  .splash__pulse,
  .splash__bar > i,
  .splash__dot {
    animation: none;
  }
  .splash__pulse {
    display: none;
  }
  .splash__bar > i {
    width: 100%;
    opacity: 0.6;
  }
  .splash__stage {
    animation: none;
    opacity: 1;
  }
}
</style>