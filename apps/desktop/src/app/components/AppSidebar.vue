<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import { NAV_ITEMS, NAV_ICONS } from '../navigation'
import type { SidebarMode } from '../navigation'
import { locale, setLocale, t } from '@/shared/i18n'

const STORAGE_KEY = 'antares.sidebar.mode'

const route = useRoute()

function readMode(): SidebarMode {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'compact' ? 'compact' : 'expanded'
  } catch {
    return 'expanded'
  }
}

const mode = ref<SidebarMode>(readMode())
const isCompact = computed(() => mode.value === 'compact')

watch(mode, (v) => {
  hideTip()
  try {
    localStorage.setItem(STORAGE_KEY, v)
  } catch {
    /* bỏ qua: private mode / storage bị chặn */
  }
})

function toggle(): void {
  mode.value = mode.value === 'expanded' ? 'compact' : 'expanded'
}

/* Vị trí item active → indicator trượt mượt giữa các mục */
const activeIndex = computed(() =>
  NAV_ITEMS.findIndex((item) => item.id === route.name),
)

/* Tooltip dùng position: fixed → không bị overflow của danh sách cắt mất */
const tip = ref<{ id: string; top: number; left: number } | null>(null)

function showTip(e: Event, id: string): void {
  if (!isCompact.value) return
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
  tip.value = { id, top: r.top + r.height / 2, left: r.right + 12 }
}
function hideTip(): void {
  tip.value = null
}
onBeforeUnmount(hideTip)

/* Điều hướng bàn phím: ↑/↓ chuyển focus giữa các mục (vòng tròn) */
function onListKeydown(e: KeyboardEvent): void {
  if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
  const links = Array.from(
    (e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('.sidebar__link'),
  )
  const i = links.indexOf(document.activeElement as HTMLElement)
  if (i < 0) return
  e.preventDefault()
  const n = links.length
  links[e.key === 'ArrowDown' ? (i + 1) % n : (i - 1 + n) % n]?.focus()
}
</script>

<template>
  <nav
    class="sidebar neu"
    :class="{ 'sidebar--compact': isCompact }"
    :aria-label="t('sidebar.aria')"
  >
    <div class="sidebar__header">
      <span
        class="sidebar__logo"
        aria-hidden="true"
      >
        <img
          class="sidebar__logo-img"
          src="/antares-logo.png"
          alt=""
          draggable="false"
        >
        <span class="sidebar__logo-dot" />
      </span>
      <Transition name="brand">
        <span
          v-if="!isCompact"
          class="sidebar__brand"
        >ANTARES</span>
      </Transition>

      <button
        class="sidebar__toggle"
        type="button"
        :aria-expanded="!isCompact"
        :title="isCompact ? t('sidebar.expand') : t('sidebar.collapse')"
        @click="toggle"
      >
        <svg
          class="sidebar__toggle-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          :style="{ transform: isCompact ? 'rotate(180deg)' : 'rotate(0deg)' }"
        >
          <path d="M11 17l-5-5 5-5M18 17l-5-5 5-5" />
        </svg>
      </button>
    </div>

    <ul
      class="sidebar__list"
      :style="{ '--idx': Math.max(activeIndex, 0) }"
      @scroll.passive="hideTip"
      @keydown="onListKeydown"
    >
      <!-- Một indicator duy nhất, trượt tới mục đang active -->
      <li
        class="sidebar__indicator"
        :class="{ 'sidebar__indicator--on': activeIndex >= 0 }"
        aria-hidden="true"
      />

      <li
        v-for="(item, i) in NAV_ITEMS"
        :key="item.id"
        class="sidebar__li"
        :style="{ '--stagger': `${Math.min(i, 10) * 28}ms` }"
      >
        <RouterLink
          :to="item.path"
          class="sidebar__link"
          :class="{ 'sidebar__link--active': route.name === item.id }"
          :aria-label="t(`nav.${item.id}`)"
          :aria-current="route.name === item.id ? 'page' : undefined"
          @mouseenter="showTip($event, item.id)"
          @focus="showTip($event, item.id)"
          @mouseleave="hideTip"
          @blur="hideTip"
        >
          <svg
            class="sidebar__icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path :d="NAV_ICONS[item.icon] ?? ''" />
          </svg>
          <span class="sidebar__label">{{ t(`nav.${item.id}`) }}</span>
        </RouterLink>
      </li>
    </ul>

    <!-- Language switcher — segmented control, thumb trượt -->
    <div
      class="sidebar__lang"
      role="group"
      aria-label="Language"
    >
      <span
        class="sidebar__lang-thumb"
        :class="{ 'sidebar__lang-thumb--en': locale === 'en' }"
        aria-hidden="true"
      />
      <button
        class="sidebar__lang-btn"
        :class="{ 'sidebar__lang-btn--on': locale === 'vi' }"
        type="button"
        :title="t('lang.vi')"
        :aria-pressed="locale === 'vi'"
        @click="setLocale('vi')"
      >
        VI
      </button>
      <button
        class="sidebar__lang-btn"
        :class="{ 'sidebar__lang-btn--on': locale === 'en' }"
        type="button"
        :title="t('lang.en')"
        :aria-pressed="locale === 'en'"
        @click="setLocale('en')"
      >
        EN
      </button>
    </div>

    <Transition name="pop">
      <span
        v-if="tip"
        class="sidebar__tooltip"
        role="tooltip"
        :style="{ top: `${tip.top}px`, left: `${tip.left}px` }"
      >{{ t(`nav.${tip.id}`) }}</span>
    </Transition>
  </nav>
</template>

<style scoped>
.sidebar {
  --ease-out: cubic-bezier(0.22, 1, 0.36, 1);
  --spring: cubic-bezier(0.34, 1.56, 0.64, 1);
  --item-h: 40px;
  --item-gap: 4px;
  --list-pad: 8px;
  /* Tông accent suy ra từ var(--accent) → đổi accent / customAccent trong Settings vẫn đúng màu */
  --a-line: color-mix(in srgb, var(--accent) 38%, transparent);
  --a-wash: color-mix(in srgb, var(--accent) 9%, transparent);

  width: var(--sidebar-w, 232px);
  display: flex;
  flex-direction: column;
  border: none;
  border-right: 1px solid rgba(255, 255, 255, 0.03);
  background:
    radial-gradient(220px 160px at 0% 0%, var(--a-wash), transparent 70%),
    var(--neu-surface);
  box-shadow:
    6px 0 16px var(--neu-dark),
    inset -1px 0 0 var(--neu-light);
  /* Nhanh hơn (280ms): đủ mượt, cảm giác phản hồi tức thì */
  transition: width 280ms var(--ease-out);
  position: relative;
  z-index: 1;
  overflow: hidden;
  will-change: width;
  contain: layout paint style;
}
.sidebar--compact {
  width: 70px;
}

/* Đường sáng mảnh trên cùng */
.sidebar::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 1px;
  background: linear-gradient(90deg, transparent, var(--a-line), transparent);
  pointer-events: none;
}

/* ───────── Header ───────── */
.sidebar__header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 16px 14px 14px 19px;
  min-height: 62px;
}
.sidebar--compact .sidebar__header {
  flex-direction: column;
  gap: 12px;
  padding: 16px 0 12px;
}
.sidebar__logo {
  width: 32px;
  height: 32px;
  display: grid;
  place-items: center;
  border-radius: var(--radius-md);
  color: var(--accent);
  background: var(--neu-surface);
  box-shadow:
    3px 3px 7px var(--neu-dark),
    -3px -3px 7px var(--neu-light),
    inset 0 1px 0 var(--glass-highlight);
  flex-shrink: 0;
  position: relative;
  transition: transform 300ms var(--spring);
}
.sidebar:hover .sidebar__logo {
  transform: rotate(-6deg) scale(1.06);
}
.sidebar__logo-img {
  width: 24px;
  height: 24px;
  border-radius: 8px;
  object-fit: cover;
  filter: drop-shadow(0 0 6px var(--accent-glow));
}
.sidebar__logo-dot {
  position: absolute;
  top: -2px;
  right: -2px;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 8px var(--accent-glow);
  animation: logo-breath 2.8s ease-in-out infinite;
}
@keyframes logo-breath {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.45;
    transform: scale(0.72);
  }
}
.sidebar__brand {
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.24em;
  white-space: nowrap;
  background: linear-gradient(100deg, var(--text-1) 40%, var(--accent) 140%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}
.brand-enter-active {
  transition: opacity 200ms ease 100ms, transform 260ms var(--ease-out) 100ms;
}
.brand-leave-active {
  position: absolute;
  transition: opacity 80ms ease;
}
.brand-enter-from,
.brand-leave-to {
  opacity: 0;
  transform: translateX(-8px);
}

/* Toggle */
.sidebar__toggle {
  margin-left: auto;
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  background: var(--neu-surface);
  border: 1px solid rgba(255, 255, 255, 0.04);
  color: var(--text-3);
  border-radius: 50%;
  cursor: pointer;
  outline: none;
  box-shadow:
    2px 2px 6px var(--neu-dark),
    -2px -2px 6px var(--neu-light);
  transition:
    box-shadow var(--motion-instant) ease,
    color var(--motion-fast) ease,
    transform 160ms var(--spring);
}
.sidebar--compact .sidebar__toggle {
  margin-left: 0;
}
.sidebar__toggle:hover {
  color: var(--accent);
  transform: scale(1.1);
}
.sidebar__toggle:active {
  transform: scale(0.92);
  box-shadow:
    inset 3px 3px 6px var(--neu-dark-strong),
    inset -3px -3px 6px var(--neu-light);
}
.sidebar__toggle:focus-visible {
  box-shadow: 0 0 0 2px var(--accent-glow), 0 0 0 1px var(--accent) inset;
}
.sidebar__toggle-icon {
  width: 14px;
  height: 14px;
  transition: transform 320ms var(--spring);
}

/* ───────── List ───────── */
.sidebar__list {
  list-style: none;
  margin: 0;
  padding: var(--list-pad);
  display: flex;
  flex-direction: column;
  gap: var(--item-gap);
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  position: relative;
  scrollbar-width: none;
  overscroll-behavior: contain;
}
.sidebar__list::-webkit-scrollbar {
  display: none;
}

/* Indicator trượt: inset pill nhuốm màu accent + rail phát sáng */
.sidebar__indicator {
  position: absolute;
  top: var(--list-pad);
  left: var(--list-pad);
  right: var(--list-pad);
  height: var(--item-h);
  border-radius: var(--radius-md);
  background:
    linear-gradient(90deg, var(--a-wash), transparent 75%),
    var(--neu-surface-inset);
  border: 1px solid rgba(0, 0, 0, 0.18);
  box-shadow:
    inset 3px 3px 8px var(--neu-dark-strong),
    inset -3px -3px 8px var(--neu-light);
  transform: translateY(calc(var(--idx, 0) * (var(--item-h) + var(--item-gap))));
  opacity: 0;
  pointer-events: none;
  will-change: transform;
  /* Trượt nhanh hơn, vẫn có độ nảy nhẹ */
  transition:
    transform 340ms var(--spring),
    opacity 160ms ease;
  list-style: none;
}
.sidebar__indicator--on {
  opacity: 1;
}
.sidebar__indicator::before {
  content: '';
  position: absolute;
  left: 0;
  top: 50%;
  width: 3px;
  height: 56%;
  border-radius: 0 3px 3px 0;
  background: linear-gradient(180deg, var(--accent), var(--a-line));
  box-shadow: 0 0 12px var(--accent-glow);
  transform: translateY(-50%);
}
.sidebar--compact .sidebar__indicator::before {
  height: 40%;
}

.sidebar__li {
  position: relative;
  z-index: 1;
  animation: li-in 360ms var(--ease-out) both;
  animation-delay: var(--stagger, 0ms);
}
@keyframes li-in {
  from {
    opacity: 0;
    transform: translateX(-12px);
  }
}

.sidebar__link {
  position: relative;
  display: flex;
  align-items: center;
  gap: 12px;
  height: var(--item-h);
  /* padding-left cố định (18px) → icon luôn nằm đúng tâm khi compact, không bị nhảy */
  padding: 0 12px 0 18px;
  border-radius: var(--radius-md);
  color: var(--text-2);
  text-decoration: none;
  font-size: 13px;
  background: transparent;
  overflow: hidden;
  outline: none;
  -webkit-tap-highlight-color: transparent;
  transition:
    background 100ms ease,
    box-shadow 100ms ease,
    color 100ms ease,
    transform 120ms var(--ease-out);
}
/* Sheen quét ngang khi hover (transform-only) */
.sidebar__link::after {
  content: '';
  position: absolute;
  inset: 0;
  pointer-events: none;
  background: linear-gradient(105deg, transparent 35%, rgba(255, 255, 255, 0.07) 50%, transparent 65%);
  transform: translateX(-130%);
  transition: transform 560ms var(--ease-out);
}
.sidebar__link:hover::after {
  transform: translateX(130%);
}
.sidebar__link:hover:not(.sidebar__link--active) {
  background: var(--neu-surface);
  color: var(--text-1);
  box-shadow:
    2px 2px 6px var(--neu-dark),
    -2px -2px 6px var(--neu-light);
}
.sidebar__link:active {
  transform: scale(0.965);
}
.sidebar__link:focus-visible {
  box-shadow: 0 0 0 2px var(--accent-glow), 0 0 0 1px var(--accent) inset;
}
.sidebar__link--active {
  color: var(--text-0, var(--text-1));
}

.sidebar__icon {
  width: 19px;
  height: 19px;
  flex-shrink: 0;
  transition:
    transform 240ms var(--spring),
    filter 100ms ease,
    color 100ms ease;
}
.sidebar__link:hover .sidebar__icon {
  transform: scale(1.14) rotate(-5deg);
  filter: drop-shadow(0 0 5px rgba(255, 255, 255, 0.18));
}
.sidebar__link--active .sidebar__icon {
  color: var(--accent);
  filter: drop-shadow(0 0 7px var(--accent-glow));
}
.sidebar__link--active:hover .sidebar__icon {
  transform: scale(1.08);
}

.sidebar__label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  /* Mở rộng: chữ hiện ngay sau khi width bắt đầu nới */
  transition:
    opacity 180ms ease 90ms,
    transform 240ms var(--ease-out) 90ms;
}
.sidebar__link--active .sidebar__label {
  font-weight: 600;
}
.sidebar--compact .sidebar__label {
  opacity: 0;
  transform: translateX(-8px);
  pointer-events: none;
  /* Thu gọn: chữ biến mất ngay lập tức */
  transition-delay: 0s;
  transition-duration: 70ms;
}

/* ───────── Tooltip (fixed) ───────── */
.sidebar__tooltip {
  position: fixed;
  transform: translateY(-50%);
  background: var(--neu-surface, #14171f);
  border: 1px solid var(--a-line);
  color: var(--text-1);
  font-size: 11px;
  font-weight: 600;
  padding: 5px 11px;
  border-radius: var(--radius-sm);
  white-space: nowrap;
  pointer-events: none;
  z-index: 60;
  box-shadow: 0 8px 20px var(--neu-dark), 0 0 14px -4px var(--accent-glow);
}
.sidebar__tooltip::before {
  content: '';
  position: absolute;
  left: -4px;
  top: 50%;
  width: 7px;
  height: 7px;
  background: var(--neu-surface, #14171f);
  border-left: 1px solid var(--a-line);
  border-bottom: 1px solid var(--a-line);
  transform: translateY(-50%) rotate(45deg);
}
.pop-enter-active,
.pop-leave-active {
  transition: opacity 100ms ease, transform 160ms var(--ease-out);
}
.pop-enter-from,
.pop-leave-to {
  opacity: 0;
  transform: translateY(-50%) translateX(-6px);
}

/* ───────── Language switcher (segmented) ───────── */
.sidebar__lang {
  --seg-w: 26px;
  position: relative;
  display: grid;
  grid-template-columns: repeat(2, var(--seg-w));
  align-self: center;
  margin: 8px 0 16px;
  padding: 3px;
  border-radius: 999px;
  background: var(--neu-surface-inset);
  box-shadow:
    inset 2px 2px 5px var(--neu-dark-strong),
    inset -2px -2px 5px var(--neu-light);
}
.sidebar__lang-thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: var(--seg-w);
  height: calc(100% - 6px);
  border-radius: 999px;
  background: var(--accent-soft);
  border: 1px solid var(--a-line);
  box-shadow: 0 0 10px var(--accent-glow);
  transition: transform 260ms var(--spring);
}
.sidebar__lang-thumb--en {
  transform: translateX(var(--seg-w));
}
.sidebar__lang-btn {
  position: relative;
  z-index: 1;
  background: none;
  border: none;
  color: var(--text-3);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.06em;
  padding: 4px 0;
  border-radius: 999px;
  cursor: pointer;
  outline: none;
  transition: color 100ms ease, transform 120ms var(--spring);
}
.sidebar__lang-btn:hover {
  color: var(--text-1);
}
.sidebar__lang-btn:active {
  transform: scale(0.9);
}
.sidebar__lang-btn--on {
  color: var(--accent);
}
.sidebar__lang-btn:focus-visible {
  box-shadow: 0 0 0 2px var(--accent-glow);
}

/* Sidebar compact: switcher xếp dọc cho vừa 70px */
.sidebar--compact .sidebar__lang {
  --seg-w: 24px;
}

/* ───────── Reduced motion ───────── */
@media (prefers-reduced-motion: reduce) {
  .sidebar,
  .sidebar * {
    animation: none !important;
    transition-duration: 1ms !important;
    transition-delay: 0s !important;
  }
}
</style>