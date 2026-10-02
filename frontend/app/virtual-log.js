/**
 * VirtualLog — console ảo (spec mục 16: "không tạo 10.000 DOM nodes").
 *
 * Chỉ render các dòng đang nhìn thấy + overscan, dùng 2 "gap" trên/dưới để
 * giữ đúng tổng chiều cao → thanh scroll vẫn đúng như list thật.
 *
 * Đặc điểm:
 * - Cao độ dòng cố định (--vlog-row) nên cuộn 10k dòng vẫn 60fps.
 * - Tự bám đáy (auto-follow) khi người dùng đang ở cuối; cuộn lên thì giữ nguyên.
 * - Cập nhật tăng dần (append) khi log chỉ nối thêm, tránh dựng lại 10k mảng.
 * - Escape HTML 1 lần lúc nạp dòng, không lặp lại mỗi frame.
 */

export const LOG_ROW_HEIGHT = 20;
export const LOG_OVERSCAN = 12;

const ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ESCAPES[c]);
}

/** Phân loại mức log để tô màu (giữ tương thích class .err/.warn/.info/.ok). */
export function classifyLine(text) {
  const s = String(text ?? '');
  if (/\b(ERROR|FATAL|CRITICAL)\b|Exception|Traceback|FAILED/.test(s)) return 'err';
  if (/\bWARN(ING)?\b/.test(s)) return 'warn';
  if (/\bINFO\b/.test(s)) return 'info';
  if (/\b(OK|SUCCESS|DONE)\b/.test(s)) return 'ok';
  return '';
}

/**
 * @param {object} opts
 * @param {HTMLElement} opts.container  phần tử .console (sẽ được quản lý nội dung)
 * @param {number} [opts.rowHeight]
 * @param {number} [opts.overscan]      số dòng đệm trên/dưới viewport
 * @param {string} [opts.emptyText]     nội dung khi chưa có dòng nào
 * @param {(text:string)=>string} [opts.classify]
 * @param {number} [opts.maxLines]      cắt bớt nếu vượt (bảo vệ bộ nhớ)
 */
export function createVirtualLog({
  container,
  rowHeight = LOG_ROW_HEIGHT,
  overscan = LOG_OVERSCAN,
  emptyText = '—',
  classify = classifyLine,
  maxLines = 20000,
} = {}) {
  if (!container) throw new Error('createVirtualLog: thiếu container');

  // Highlight từ khoá filter: hàm biến text đã thành html có <mark>.
  // null = không highlight. Đổi term chỉ cần vẽ lại (không đổi dữ liệu).
  let highlightFn = null;   // (html)=>html

  container.classList.add('console', 'vlog');
  container.style.setProperty('--vlog-row', `${rowHeight}px`);
  container.innerHTML = '';

  let rows = [];          // { html, cls } — đã escape sẵn
  let raw = [];           // text gốc (để so khớp khi append)
  let firstRendered = -1;
  let lastRendered = -1;
  let stick = true;       // đang bám đáy?
  let raf = 0;
  let destroyed = false;

  const onScroll = () => {
    stick = atBottom();
    schedule();
  };
  container.addEventListener('scroll', onScroll, { passive: true });

  let resizeObserver = null;
  if (typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(() => { invalidate(); schedule(); });
    resizeObserver.observe(container);
  }

  function atBottom() {
    return container.scrollHeight - container.scrollTop - container.clientHeight <= rowHeight * 2;
  }

  function invalidate() {
    firstRendered = -1;
    lastRendered = -1;
  }

  function schedule() {
    if (raf || destroyed) return;
    raf = requestAnimationFrame(() => { raf = 0; paint(); });
  }

  function paint(force = false) {
    if (destroyed) return;
    const total = rows.length;

    if (!total) {
      container.innerHTML = `<div class="vlog-empty">${escapeHtml(emptyText)}</div>`;
      invalidate();
      return;
    }

    const viewH = container.clientHeight || rowHeight * 12;
    const first = Math.max(0, Math.floor(container.scrollTop / rowHeight) - overscan);
    const last = Math.min(total, first + Math.ceil(viewH / rowHeight) + overscan * 2);

    if (!force && first === firstRendered && last === lastRendered) return;
    firstRendered = first;
    lastRendered = last;

    let html = `<div class="vlog-gap" style="height:${first * rowHeight}px"></div>`;
    for (let i = first; i < last; i++) {
      const r = rows[i];
      html += `<div class="vlog-line${r.cls ? ` ${r.cls}` : ''}">${r.html}</div>`;
    }
    html += `<div class="vlog-gap" style="height:${Math.max(0, (total - last) * rowHeight)}px"></div>`;
    container.innerHTML = html;
  }

  /**
   * Đồng bộ DOM + vị trí scroll sau khi dữ liệu đổi.
   * - Đang bám đáy: vẽ lại rồi nhảy xuống cuối (1 lần ép layout, không đọc scrollHeight).
   * - Đang đọc (không bám): chỉ vẽ nếu cửa sổ thật sự đổi -> nhịp poll không nháy DOM.
   */
  function settle() {
    if (!stick) {
      paint();
      return;
    }
    paint(true);
    const max = rows.length * rowHeight - container.clientHeight;
    if (max >= 0) container.scrollTop = max;
    paint(true);
  }

  function toRows(lines) {
    return lines.map((entry) => {
      // UIX v3: nhận cả object {text, level} (log.lines v2) hoặc string thuần.
      const isObj = entry && typeof entry === 'object';
      const s = String(isObj ? entry.text : entry ?? '');
      const meta = isObj ? String(entry.level || '') : '';
      const cls = meta
        ? ({ ERROR: 'err', FATAL: 'err', CRITICAL: 'err',
             WARNING: 'warn', WARN: 'warn', INFO: 'info' }[meta.toUpperCase()] || '')
        : classify(s);
      const html = highlightFn ? highlightFn(escapeHtml(s)) : escapeHtml(s);
      return { html, cls };
    });
  }

  function trim() {
    if (rows.length <= maxLines) return;
    const drop = rows.length - maxLines;
    rows = rows.slice(drop);
    raw = raw.slice(drop);
  }

  /** Thay toàn bộ nội dung. */
  function setLines(lines) {
    raw = (lines || []).map((l) => String(l ?? ''));
    rows = toRows(raw);
    trim();
    invalidate();
    settle();
  }

  /**
   * Cập nhật thông minh: nếu chỉ nối thêm (log append-only) thì chỉ escape phần mới.
   * Trả về true nếu đi theo đường nhanh.
   */
  function updateLines(lines) {
    const next = lines || [];
    const canAppend = raw.length > 0
      && next.length > raw.length
      && next[0] === raw[0]
      && next[raw.length - 1] === raw[raw.length - 1];

    if (!canAppend) { setLines(next); return false; }

    const appended = next.slice(raw.length);
    for (const entry of appended) {
      const isObj = entry && typeof entry === 'object';
      const s = String(isObj ? entry.text : entry ?? '');
      const meta = isObj ? String(entry.level || '') : '';
      const cls = meta
        ? ({ ERROR: 'err', FATAL: 'err', CRITICAL: 'err',
             WARNING: 'warn', WARN: 'warn', INFO: 'info' }[meta.toUpperCase()] || '')
        : classify(s);
      const html = highlightFn ? highlightFn(escapeHtml(s)) : escapeHtml(s);
      raw.push(s);
      rows.push({ html, cls });
    }
    trim();
    settle();
    return true;
  }

  function clear() {
    raw = [];
    rows = [];
    stick = true;
    invalidate();
    paint(true);
  }

  /**
   * Đặt/huỷ từ khoá highlight (case-insensitive, escape-safe vì chạy trên HTML
   * ĐÃ escape — pattern cũng được escape). Đổi từ khoá -> vẽ lại toàn bộ.
   */
  function setHighlight(term) {
    const t = String(term ?? '').trim();
    if (!t) { highlightFn = null; invalidate(); settle(); return; }
    const safe = t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const re = new RegExp(`(${safe})`, 'gi');
    // Chỉ bọc <mark> cho text nằm NGOÀI tag đã có (class/không đè style mức)
    highlightFn = (html) => html.replace(re, '<mark>$1</mark>');
    invalidate();
    settle();
  }

  function scrollToBottom() {
    stick = true;
    container.scrollTop = container.scrollHeight;
    paint(true);
  }

  function refresh() {
    invalidate();
    settle();
  }

  function destroy() {
    destroyed = true;
    if (raf) cancelAnimationFrame(raf);
    container.removeEventListener('scroll', onScroll);
    resizeObserver?.disconnect();
  }

  paint(true);

  return {
    setLines,
    updateLines,
    clear,
    refresh,
    setHighlight,
    scrollToBottom,
    destroy,
    classify,
    /** Số dòng đang giữ. */
    get count() { return rows.length; },
    /** Người dùng có đang ở cuối console? */
    get following() { return stick; },
    get rowHeight() { return rowHeight; },
  };
}
