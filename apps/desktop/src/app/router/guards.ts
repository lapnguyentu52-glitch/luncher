const NAVIGATION_BUDGET_MS = 100

const bootStart = Date.now()

export function navigationGuards(to: { name?: unknown }): boolean {
  if (to.name === 'splash') {
    // Không cho quay lại splash sau khi đã vào app
    return Date.now() - bootStart < 1000
  }
  // (Instrumentation: navigation budget §33 — hiện đang no-op, hook ở Batch 3)
  void NAVIGATION_BUDGET_MS
  return true
}
