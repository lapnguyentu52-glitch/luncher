/**
 * Bridge — điểm duy nhất frontend gọi backend (spec mục 202).
 * Không component nào được gọi window.pywebview.api trực tiếp.
 */

const pending = [];

async function waitForApi(timeoutMs = 8000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (window.pywebview?.api) return window.pywebview.api;
    await new Promise(r => setTimeout(r, 50));
  }
  return null;
}

export async function call(command, ...args) {
  const api = await waitForApi();
  if (!api) {
    return { ok: false, error: { code: 'BRIDGE_UNAVAILABLE', message: 'Backend bridge not ready' } };
  }
  const method = command.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
  const fn = api[command] || api[method];
  if (typeof fn !== 'function') {
    return { ok: false, error: { code: 'UNKNOWN_COMMAND', message: `Unknown command: ${command}` } };
  }
  try {
    return await fn(...args);
  } catch (e) {
    return { ok: false, error: { code: 'BRIDGE_ERROR', message: String(e) } };
  }
}

/** Subscribe event từ backend (pywebview custom event). */
export function onEvent(name, handler) {
  window.addEventListener('pywebview-event', (e) => {
    if (!name || e.detail?.event === name) handler(e.detail);
  });
}
