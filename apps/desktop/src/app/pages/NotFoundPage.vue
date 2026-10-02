<script setup lang="ts">
import { useRoute, useRouter } from 'vue-router'

const route = useRoute()
const router = useRouter()

function goBack(): void {
  // Có lịch sử thì quay lại, không thì về Dashboard
  if (window.history.length > 1) router.back()
  else router.push('/')
}
</script>

<template>
  <section
    class="not-found"
    aria-labelledby="nf-title"
  >
    <div class="not-found__stage">
      <!-- 404 — chữ 0 là quỹ đạo của một ngôi sao (Antares) -->
      <h1
        id="nf-title"
        class="not-found__code"
        aria-label="404"
      >
        <span
          class="not-found__digit"
          aria-hidden="true"
        >4</span>
        <svg
          class="not-found__orbit"
          viewBox="0 0 120 120"
          fill="none"
          aria-hidden="true"
        >
          <circle
            class="not-found__ring"
            cx="60"
            cy="60"
            r="50"
          />
          <circle
            class="not-found__ring not-found__ring--dashed"
            cx="60"
            cy="60"
            r="36"
          />
          <g class="not-found__sweep">
            <circle
              class="not-found__moon"
              cx="60"
              cy="10"
              r="4"
            />
          </g>
          <circle
            class="not-found__halo"
            cx="60"
            cy="60"
            r="14"
          />
          <circle
            class="not-found__core"
            cx="60"
            cy="60"
            r="7"
          />
        </svg>
        <span
          class="not-found__digit"
          aria-hidden="true"
        >4</span>
      </h1>

      <p class="not-found__reason">
        Không có workspace nào khớp địa chỉ này.
      </p>
      <code
        class="not-found__path"
        :title="route.fullPath"
      >{{ route.fullPath }}</code>

      <div class="not-found__actions">
        <RouterLink
          to="/"
          class="not-found__action not-found__action--primary"
        >
          Về Dashboard
        </RouterLink>
        <RouterLink
          to="/play"
          class="not-found__action"
        >
          Mở Play
        </RouterLink>
        <button
          class="not-found__action not-found__action--ghost"
          type="button"
          @click="goBack"
        >
          Quay lại
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.not-found {
  --ease-out: cubic-bezier(0.22, 1, 0.36, 1);
  --spring: cubic-bezier(0.34, 1.56, 0.64, 1);

  position: relative;
  display: grid;
  place-items: center;
  height: 100%;
  padding: 24px;
  overflow: hidden;
}

/* Vầng sáng tĩnh phía sau — không tốn hiệu năng */
.not-found::before {
  content: '';
  position: absolute;
  width: min(560px, 90vw);
  aspect-ratio: 1;
  border-radius: 50%;
  background: radial-gradient(
    circle,
    var(--accent-glow, rgba(255, 92, 71, 0.28)) 0%,
    transparent 62%
  );
  opacity: 0.35;
  pointer-events: none;
}

.not-found__stage {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 14px;
}

/* ───────── 4 [quỹ đạo] 4 ───────── */
.not-found__code {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.04em;
  margin: 0;
  font-size: clamp(84px, 16vw, 148px);
  font-weight: 800;
  line-height: 1;
  letter-spacing: -0.04em;
}
.not-found__digit {
  background: linear-gradient(180deg, var(--text-1), var(--text-3) 85%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
  filter: drop-shadow(0 6px 14px var(--neu-dark, rgba(0, 0, 0, 0.5)));
  animation: rise 700ms var(--ease-out) both;
}
.not-found__digit:last-child {
  animation-delay: 140ms;
}

.not-found__orbit {
  width: 0.86em;
  height: 0.86em;
  flex-shrink: 0;
  overflow: visible;
  animation: rise 700ms var(--ease-out) 70ms both;
}
.not-found__ring {
  stroke: var(--text-3);
  stroke-width: 1.6;
  opacity: 0.55;
}
.not-found__ring--dashed {
  stroke-dasharray: 2 7;
  stroke-linecap: round;
  opacity: 0.4;
  transform-box: fill-box;
  transform-origin: center;
  animation: spin 40s linear infinite reverse;
}
.not-found__sweep {
  transform-box: view-box;
  transform-origin: 60px 60px;
  animation: spin 9s linear infinite;
}
.not-found__moon {
  fill: var(--text-1);
  filter: drop-shadow(0 0 4px rgba(255, 255, 255, 0.5));
}
.not-found__halo {
  fill: var(--accent-soft, rgba(255, 92, 71, 0.18));
  transform-box: fill-box;
  transform-origin: center;
  animation: breath 3.2s ease-in-out infinite;
}
.not-found__core {
  fill: var(--accent);
  filter: drop-shadow(0 0 10px var(--accent-glow, rgba(255, 92, 71, 0.6)));
}

@keyframes rise {
  from {
    opacity: 0;
    transform: translateY(14px) scale(0.96);
  }
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
@keyframes breath {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.45;
    transform: scale(1.35);
  }
}

/* ───────── Nội dung ───────── */
.not-found__reason {
  margin: 8px 0 0;
  max-width: 36ch;
  font-size: 15px;
  line-height: 1.5;
  color: var(--text-2);
  animation: rise 600ms var(--ease-out) 260ms both;
}
.not-found__path {
  display: block;
  max-width: min(420px, 80vw);
  padding: 5px 14px;
  border-radius: 999px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  background: var(--neu-surface-inset, var(--surface-2));
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong, rgba(0, 0, 0, 0.4)),
    inset -2px -2px 5px var(--neu-light, rgba(255, 255, 255, 0.04));
  animation: rise 600ms var(--ease-out) 320ms both;
}

/* ───────── Nút ───────── */
.not-found__actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
  margin-top: 14px;
  animation: rise 600ms var(--ease-out) 400ms both;
}
.not-found__action {
  position: relative;
  display: inline-flex;
  align-items: center;
  padding: 10px 20px;
  border-radius: var(--radius-md, 10px);
  border: 1px solid rgba(255, 255, 255, 0.04);
  background: var(--neu-surface, var(--surface-2));
  color: var(--text-2);
  font: inherit;
  font-size: 13px;
  font-weight: 500;
  text-decoration: none;
  cursor: pointer;
  outline: none;
  overflow: hidden;
  box-shadow:
    4px 4px 10px var(--neu-dark, rgba(0, 0, 0, 0.45)),
    -3px -3px 8px var(--neu-light, rgba(255, 255, 255, 0.04));
  transition:
    transform 220ms var(--spring),
    box-shadow var(--motion-fast, 160ms) ease,
    color var(--motion-fast, 160ms) ease,
    background var(--motion-fast, 160ms) ease;
}
/* Sheen quét ngang khi hover */
.not-found__action::after {
  content: '';
  position: absolute;
  inset: 0;
  pointer-events: none;
  background: linear-gradient(105deg, transparent 35%, rgba(255, 255, 255, 0.08) 50%, transparent 65%);
  transform: translateX(-130%);
  transition: transform 700ms var(--ease-out);
}
.not-found__action:hover {
  transform: translateY(-2px);
  color: var(--text-1);
}
.not-found__action:hover::after {
  transform: translateX(130%);
}
.not-found__action:active {
  transform: translateY(0) scale(0.97);
  box-shadow:
    inset 3px 3px 7px var(--neu-dark-strong, rgba(0, 0, 0, 0.5)),
    inset -3px -3px 7px var(--neu-light, rgba(255, 255, 255, 0.04));
}
.not-found__action:focus-visible {
  box-shadow: 0 0 0 2px var(--accent-glow, rgba(255, 92, 71, 0.5));
}

.not-found__action--primary {
  color: #fff;
  border-color: rgba(255, 255, 255, 0.14);
  background: linear-gradient(180deg, var(--accent), rgba(255, 92, 71, 0.78));
  box-shadow:
    0 6px 18px var(--accent-glow, rgba(255, 92, 71, 0.4)),
    inset 0 1px 0 rgba(255, 255, 255, 0.28);
}
.not-found__action--primary:hover {
  color: #fff;
  box-shadow:
    0 10px 26px var(--accent-glow, rgba(255, 92, 71, 0.5)),
    inset 0 1px 0 rgba(255, 255, 255, 0.32);
}
.not-found__action--primary:active {
  box-shadow:
    0 2px 8px var(--accent-glow, rgba(255, 92, 71, 0.4)),
    inset 0 2px 6px rgba(0, 0, 0, 0.25);
}

.not-found__action--ghost {
  background: transparent;
  border-color: transparent;
  box-shadow: none;
  color: var(--text-3);
}
.not-found__action--ghost:hover {
  background: var(--neu-surface, var(--surface-2));
  box-shadow:
    2px 2px 6px var(--neu-dark, rgba(0, 0, 0, 0.45)),
    -2px -2px 6px var(--neu-light, rgba(255, 255, 255, 0.04));
}

/* ───────── Reduced motion ───────── */
@media (prefers-reduced-motion: reduce) {
  .not-found *,
  .not-found *::after {
    animation: none !important;
    transition-duration: 1ms !important;
  }
}
</style>