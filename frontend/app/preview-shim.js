/**
 * Preview shim — chạy UI trong TRÌNH DUYỆT qua preview_server.py (không pywebview).
 *
 * Cung cấp 2 thứ app cần:
 * 1. window.pywebview.api[command](...args) — POST /bridge tới mock backend.
 * 2. Kênh đẩy event: SSE /events -> window.__antaresEvent(batch) (giống EventBridge).
 *
 * Shim chỉ được chèn bởi preview_server (index.html gốc không đụng tới).
 * Guard headless: UI smoke import mọi file app/*.js trong Node (không có window).
 */
(async function () {
  'use strict';

  if (typeof window === 'undefined') return;   // Node headless — không làm gì

  // Đợi event shim sẵn sàng TRƯỚC khi app boot (bridge.waitForApi cũng chờ).
  window.pywebview = {
    api: new Proxy({}, {
      get(_t, command) {
        if (typeof command !== 'string') return undefined;
        return async (...args) => {
          try {
            const res = await fetch('/bridge', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ command, args }),
            });
            return await res.json();
          } catch (e) {
            return { ok: false, error: { code: 'BRIDGE_ERROR', message: String(e) } };
          }
        };
      },
    }),
  };

  // Kênh đẩy event: SSE -> cùng đường vào với EventBridge (window.__antaresEvent).
  if (typeof EventSource !== 'undefined') {
    const es = new EventSource('/events');
    es.onmessage = (e) => {
      try {
        const batch = JSON.parse(e.data);
        window.__antaresEvent && window.__antaresEvent(batch);
      } catch { /* bỏ batch hỏng */ }
    };
  }

  // Báo hiệu sẵn sàng (pywebview really dùng event DOMContentLoaded -> API_READY)
  window.dispatchEvent(new CustomEvent('pywebviewready'));
})();
