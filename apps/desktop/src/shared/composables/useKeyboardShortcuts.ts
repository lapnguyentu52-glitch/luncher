export interface ShortcutCombo {
  key: string
  ctrl?: boolean
  shift?: boolean
  alt?: boolean
  handler: () => void
}

export function useKeyboardShortcuts() {
  const combos: ShortcutCombo[] = []

  function onKeyDown(e: KeyboardEvent): void {
    for (const combo of combos) {
      const ctrlOk = combo.ctrl ? e.ctrlKey || e.metaKey : !e.ctrlKey && !e.metaKey
      const shiftOk = combo.shift ? e.shiftKey : !e.shiftKey
      const altOk = combo.alt ? e.altKey : !e.altKey
      if (e.key.toLowerCase() === combo.key && ctrlOk && shiftOk && altOk) {
        e.preventDefault()
        combo.handler()
        return
      }
    }
  }

  if (typeof window !== 'undefined') {
    window.addEventListener('keydown', onKeyDown)
  }

  function register(combo: ShortcutCombo): void {
    combos.push(combo)
  }

  function unregister(combo: ShortcutCombo): void {
    const index = combos.indexOf(combo)
    if (index >= 0) combos.splice(index, 1)
  }

  function dispose(): void {
    if (typeof window !== 'undefined') window.removeEventListener('keydown', onKeyDown)
  }

  return { register, unregister, dispose }
}
