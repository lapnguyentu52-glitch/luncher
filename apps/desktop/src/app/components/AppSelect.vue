<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'

/**
 * AppSelect — custom dropdown thay native <select>: dark premium nhất quán,
 * không trắng xóa; giữ keyboard nav + aria. Drop-in: v-model + options
 * [{value,label}] hoặc slot tự render option.
 */
export interface AppSelectOption {
  value: string
  label: string
}

const props = withDefaults(
  defineProps<{
    modelValue: string
    options: AppSelectOption[]
    placeholder?: string
    disabled?: boolean
    ariaLabel?: string
    /** Kéo dropdown lên trên nếu thiếu chỗ bên dưới */
    placement?: 'auto' | 'top'
  }>(),
  { placeholder: '', disabled: false, ariaLabel: '', placement: 'auto' },
)

const emit = defineEmits<{
  'update:modelValue': [value: string]
  change: [value: string]
}>()

const open = ref(false)
const highlight = ref(0)
const rootEl = ref<HTMLElement | null>(null)

const selectedLabel = computed(
  () => props.options.find((o) => o.value === props.modelValue)?.label ?? props.placeholder,
)
const hasValue = computed(() => props.options.some((o) => o.value === props.modelValue))

const openUp = ref(false)
function toggle(): void {
  if (props.disabled) return
  if (!open.value) {
    // Đo viewport — thiếu chỗ bên dưới thì mở lên
    if (props.placement === 'top') {
      openUp.value = true
    } else {
      const rect = rootEl.value?.getBoundingClientRect()
      openUp.value = rect ? rect.bottom + 260 > window.innerHeight : false
    }
    highlight.value = Math.max(
      0,
      props.options.findIndex((o) => o.value === props.modelValue),
    )
  }
  open.value = !open.value
}

function choose(value: string): void {
  emit('update:modelValue', value)
  emit('change', value)
  open.value = false
  // focus về button sau khi chọn (a11y)
  rootEl.value?.querySelector<HTMLButtonElement>('.app-select__trigger')?.focus()
}

function onKeydown(e: KeyboardEvent): void {
  if (props.disabled) return
  switch (e.key) {
    case 'Enter':
    case ' ':
      e.preventDefault()
      if (open.value) {
        const picked = props.options[highlight.value]
        if (picked !== undefined) choose(picked.value)
      } else {
        toggle()
      }
      break
    case 'Escape':
      if (open.value) {
        e.stopPropagation()
        open.value = false
      }
      break
    case 'ArrowDown':
      e.preventDefault()
      if (!open.value) toggle()
      else highlight.value = Math.min(highlight.value + 1, props.options.length - 1)
      break
    case 'ArrowUp':
      e.preventDefault()
      if (!open.value) toggle()
      else highlight.value = Math.max(highlight.value - 1, 0)
      break
    case 'Home':
      if (open.value) {
        e.preventDefault()
        highlight.value = 0
      }
      break
    case 'End':
      if (open.value) {
        e.preventDefault()
        highlight.value = props.options.length - 1
      }
      break
    default:
      break
  }
}

// Đóng khi click ra ngoài
function onDocClick(e: MouseEvent): void {
  if (rootEl.value && !rootEl.value.contains(e.target as Node)) open.value = false
}
watch(open, (v) => {
  if (v) document.addEventListener('click', onDocClick)
  else document.removeEventListener('click', onDocClick)
})
onBeforeUnmount(() => document.removeEventListener('click', onDocClick))
</script>

<template>
  <div
    ref="rootEl"
    class="app-select"
    :class="{ 'app-select--open': open, 'app-select--disabled': disabled }"
  >
    <button
      type="button"
      class="app-select__trigger"
      :disabled="disabled"
      role="combobox"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-label="ariaLabel || undefined"
      @click="toggle"
      @keydown="onKeydown"
    >
      <span
        class="app-select__value"
        :class="{ 'app-select__value--placeholder': !hasValue }"
      >{{ selectedLabel }}</span>
      <svg
        class="app-select__chevron"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M6 9l6 6 6-6" />
      </svg>
    </button>

    <Transition name="appsel">
      <ul
        v-if="open"
        class="app-select__list"
        :class="{ 'app-select__list--up': openUp }"
        role="listbox"
        :aria-label="ariaLabel || undefined"
      >
        <li
          v-for="(o, i) in options"
          :key="o.value"
          role="option"
          :aria-selected="o.value === modelValue"
          class="app-select__option"
          :class="{
            'app-select__option--selected': o.value === modelValue,
            'app-select__option--highlight': i === highlight,
          }"
          @click="choose(o.value)"
          @mousemove="highlight = i"
        >
          <span class="app-select__check">
            <svg
              v-if="o.value === modelValue"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M5 13l4 4L19 7" />
            </svg>
          </span>
          {{ o.label }}
        </li>
      </ul>
    </Transition>
  </div>
</template>

<style scoped>
.app-select {
  position: relative;
  width: 100%;
  font-size: 12px;
}
.app-select--disabled {
  opacity: 0.5;
  cursor: default;
}

/* Closed: #14171F + border #343A46 + radius 10 + 36px */
.app-select__trigger {
  width: 100%;
  height: 36px;
  display: flex;
  align-items: center;
  gap: 8px;
  background: #14171f;
  border: 1px solid #343a46;
  border-radius: 10px;
  color: #f5f7fa;
  padding: 0 10px 0 12px;
  cursor: pointer;
  transition:
    border-color 160ms ease,
    box-shadow 160ms ease,
    background 160ms ease;
}
.app-select__trigger:hover:not(:disabled) {
  border-color: #3a414f;
  background: #171b24;
}
.app-select__trigger:focus-visible {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-glow);
}

.app-select__value {
  flex: 1;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #f5f7fa;
}
.app-select__value--placeholder {
  color: var(--text-3);
}

/* Custom chevron — xoay khi mở */
.app-select__chevron {
  width: 14px;
  height: 14px;
  color: var(--text-3);
  flex-shrink: 0;
  transition: transform 180ms cubic-bezier(0.34, 1.56, 0.64, 1);
}
.app-select--open .app-select__chevron {
  transform: rotate(180deg);
}

/* Open list: #14171F, border #3A414F, không trắng */
.app-select__list {
  position: absolute;
  top: calc(100% + 5px);
  left: 0;
  right: 0;
  z-index: 70;
  list-style: none;
  margin: 0;
  padding: 5px;
  background: #14171f;
  border: 1px solid #3a414f;
  border-radius: 10px;
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.55);
  max-height: 260px;
  overflow-y: auto;
}
.app-select__list--up {
  top: auto;
  bottom: calc(100% + 5px);
}

/* Option: #E5E7EB, hover/selected #1E2633 */
.app-select__option {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 9px;
  border-radius: 7px;
  color: #e5e7eb;
  cursor: pointer;
  transition: background 150ms ease, color 150ms ease;
}
.app-select__option--highlight {
  background: #1e2633;
}
.app-select__option--selected {
  color: #f5f7fa;
  font-weight: 600;
}
.app-select__option--selected.app-select__option--highlight {
  background: #1e2633;
}

.app-select__check {
  width: 14px;
  height: 14px;
  display: inline-grid;
  place-items: center;
  flex-shrink: 0;
  color: var(--accent);
}
.app-select__check svg {
  width: 12px;
  height: 12px;
}

/* Transition */
.appsel-enter-active,
.appsel-leave-active {
  transition: opacity 160ms ease, transform 180ms cubic-bezier(0.34, 1.56, 0.64, 1);
}
.appsel-enter-from,
.appsel-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
}

@media (prefers-reduced-motion: reduce) {
  .appsel-enter-active,
  .appsel-leave-active,
  .app-select__chevron {
    transition: none;
  }
}
</style>
