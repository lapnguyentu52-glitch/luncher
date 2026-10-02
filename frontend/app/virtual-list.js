/**
 * VirtualList — ảo hoá danh sách/grid (spec mục 16 "không tạo 10.000 DOM nodes",
 * mục 27 VirtualList).
 *
 * Hai chế độ:
 * - `scroller` (khuyến nghị): mount nằm trong một scroll container khác (ví dụ
 *   `#content`). Cuộn trang tự nhiên, không có scrollbar lồng nhau; cửa sổ dòng
 *   được tính từ vị trí mount so với viewport của scroller.
 * - `scroller = null`: mount tự cuộn (cần class `.vlist-scroll` + chiều cao).
 *
 * Cao độ dòng cố định nên cuộn vài nghìn mục vẫn mượt; grid được ảo hoá theo
 * hàng (mỗi hàng gồm `columns` mục).
 */

export const VLIST_OVERSCAN = 6;

/**
 * @param {object} opts
 * @param {HTMLElement} opts.mount        phần tử được quản lý nội dung (giữ nguyên identity)
 * @param {HTMLElement|null} [opts.scroller] scroll container; null = mount tự cuộn
 * @param {number} opts.rowHeight         chiều cao 1 hàng (px)
 * @param {number} [opts.gap]             khoảng cách dọc/ngang giữa các mục (px)
 * @param {number} [opts.columns]         số cột cố định
 * @param {number} [opts.minColumnWidth]  >0: tự tính số cột theo bề rộng mount
 * @param {number} [opts.overscan]        số hàng đệm trên/dưới
 * @param {(items:any[], startIndex:number)=>string} opts.renderRow
 * @param {string} [opts.rowClass]  class thêm cho mỗi hàng (ví dụ 'vlist-sep-row')
 * @param {string} [opts.emptyHtml]
 * @param {(root:HTMLElement, info:object)=>void} [opts.onRender]  hook sau mỗi lần vẽ
 */
export function createVirtualList({
  mount,
  scroller = null,
  rowHeight,
  gap = 0,
  columns = 1,
  minColumnWidth = 0,
  overscan = VLIST_OVERSCAN,
  renderRow,
  rowClass = '',
  emptyHtml = '',
  onRender = null,
} = {}) {
  if (!mount) throw new Error('createVirtualList: thiếu mount');
  if (typeof renderRow !== 'function') throw new Error('createVirtualList: thiếu renderRow');
  if (!(rowHeight > 0)) throw new Error('createVirtualList: rowHeight phải > 0');

  const own = !scroller || scroller === mount;
  const scrollEl = scroller || mount;
  // `window` luôn có trong browser; guard để module chạy được cả trong test Node.
  const resizeTarget = typeof window !== 'undefined' ? window : null;

  let items = [];
  let cols = Math.max(1, Math.floor(columns) || 1);
  let firstRow = -1;
  let lastRow = -1;
  let raf = 0;
  let destroyed = false;
  let lastWidth = -1;
  let resizeObserver = null;

  mount.classList.add('vlist');
  if (own) mount.classList.add('vlist-scroll');

  const pitch = () => rowHeight + gap;
  const totalRows = () => Math.ceil(items.length / cols) || 0;

  /* ---------------- đo lường ---------------- */

  function metrics() {
    if (own) {
      return { offset: mount.scrollTop, viewH: mount.clientHeight || 400, width: mount.clientWidth };
    }
    // mount nằm trong scroller: khoảng cách từ đỉnh viewport scroller tới đỉnh mount
    const mr = mount.getBoundingClientRect();
    const sr = scrollEl.getBoundingClientRect();
    return { offset: sr.top - mr.top, viewH: sr.height || 400, width: mr.width };
  }

  function resolveColumns(width) {
    if (!(minColumnWidth > 0) || width <= 0) return Math.max(1, Math.floor(columns) || 1);
    return Math.max(1, Math.floor((width + gap) / (minColumnWidth + gap)));
  }

  /* ---------------- vòng đời ---------------- */

  const onScroll = () => schedule();

  function invalidate() {
    firstRow = -1;
    lastRow = -1;
  }

  function schedule() {
    if (raf || destroyed) return;
    raf = requestAnimationFrame(() => { raf = 0; paint(); });
  }

  function paint(force = false) {
    if (destroyed) return;

    if (!items.length) {
      mount.innerHTML = emptyHtml
        ? `<div class="vlist-empty">${emptyHtml}</div>`
        : '<div class="vlist-empty">—</div>';
      invalidate();
      onRender?.(mount, { count: 0, total: 0, columns: cols });
      return;
    }

    const { offset, viewH, width } = metrics();

    // Số cột phải chốt TRƯỚC khi tính số hàng, nếu không cửa sổ sẽ lệch
    // khi grid đổi số cột theo bề rộng (xem tests/js/virtual_list.test.mjs).
    const nextCols = resolveColumns(width);
    if (nextCols !== cols) { cols = nextCols; invalidate(); force = true; }
    lastWidth = width;

    const total = totalRows();
    const p = pitch();
    const first = Math.max(0, Math.min(total - 1, Math.floor(Math.max(0, offset) / p) - overscan));
    const visible = Math.ceil(viewH / p) + overscan * 2;
    const last = Math.min(total, first + visible);

    if (!force && first === firstRow && last === lastRow) return;
    firstRow = first;
    lastRow = last;

    const gridStyle = cols > 1 ? `;grid-template-columns:repeat(${cols},minmax(0,1fr));gap:${gap}px` : '';
    const shapeClass = cols > 1 ? 'vlist-grid-row' : 'vlist-list-row';
    const extraClass = rowClass ? ` ${rowClass}` : '';
    const mb = gap ? `;margin-bottom:${gap}px` : '';

    let html = `<div class="vlist-pad" style="height:${first * p}px"></div>`;
    for (let r = first; r < last; r++) {
      const startIndex = r * cols;
      const slice = items.slice(startIndex, startIndex + cols);
      html += `<div class="vlist-row ${shapeClass}${extraClass}" data-row="${r}"`
        + ` style="height:${rowHeight}px${mb}${gridStyle}">`
        + renderRow(slice, startIndex)
        + '</div>';
    }
    html += `<div class="vlist-pad" style="height:${Math.max(0, (total - last) * p)}px"></div>`;
    mount.innerHTML = html;

    onRender?.(mount, { count: last - first, total, columns: cols, first, last });
  }

  /* ---------------- API ---------------- */

  function setItems(next) {
    items = Array.isArray(next) ? next : [];
    invalidate();
    paint(true);
  }

  function refresh() {
    invalidate();
    paint(true);
  }

  function destroy() {
    destroyed = true;
    if (raf) cancelAnimationFrame(raf);
    scrollEl.removeEventListener('scroll', onScroll);
    resizeTarget?.removeEventListener('resize', onScroll);
    resizeObserver?.disconnect();
  }

  /* ---------------- listeners ---------------- */

  scrollEl.addEventListener('scroll', onScroll, { passive: true });
  resizeTarget?.addEventListener('resize', onScroll);

  if (typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver((entries) => {
      const w = Math.round(entries[0]?.contentRect?.width ?? -1);
      if (w === lastWidth) return; // chỉ quan tâm bề rộng, tránh vòng lặp do chiều cao
      invalidate();
      schedule();
    });
    resizeObserver.observe(mount);
  }

  paint(true);

  return {
    setItems,
    refresh,
    destroy,
    render: (force = true) => paint(force),
    get count() { return items.length; },
    get columns() { return cols; },
    get mountedRows() { return Math.max(0, lastRow - firstRow); },
    get el() { return mount; },
  };
}
