/**
 * Chart — sparkline/area trên canvas (spec 3.0 mục 27).
 *
 * - Không thư viện ngoài; chỉ vẽ khi có mẫu mới (redraw theo data change,
 *   KHÔNG setInterval render — mục 79).
 * - DPR-aware để nét trên màn retina/scale Windows.
 * - Devicepixelratio thay đổi -> vẽ lại lần gọi kế tiếp (không listen resize
 *   để tránh leak; view gọi resize() khi cần).
 */

export class Sparkline {
  /**
   * @param {HTMLCanvasElement} canvas
   * @param {{color?: string, fill?: boolean, max?: number, min?: number, unit?: string}} [opts]
   */
  constructor(canvas, opts = {}) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.opts = opts;
    this._data = [];
    this._dpr = 0;
  }

  /** Cập nhật dữ liệu + vẽ lại. Gọi CHỈ khi data đổi. */
  setData(data) {
    this._data = Array.isArray(data) ? data : [];
    this.draw();
  }

  resize() {
    this._dpr = 0; // ép vẽ lại với kích thước mới
    this.draw();
  }

  draw() {
    const { canvas, ctx, opts } = this;
    const rect = canvas.getBoundingClientRect();
    if (rect.width < 4 || rect.height < 4) return;

    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(rect.width * dpr);
    const h = Math.round(rect.height * dpr);
    if (this._dpr !== dpr || canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
      this._dpr = dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, rect.width, rect.height);

    const data = this._data;
    if (!data.length) return;

    const css = getComputedStyle(document.documentElement);
    const color = opts.color || css.getPropertyValue('--accent-0').trim() || '#e4573d';
    const grid = css.getPropertyValue('--border-0').trim() || 'rgba(255,255,255,.07)';

    const pad = 2;
    const iw = rect.width - pad * 2;
    const ih = rect.height - pad * 2;

    // Auto-scale: max = max dữ liệu (trừ khi opts.max cố định), min từ opts.
    let max = opts.max;
    if (max == null) {
      max = 0;
      for (const [, v] of data) if (v > max) max = v;
      max = max <= 0 ? 1 : max * 1.15;
    }
    const min = opts.min ?? 0;

    const t0 = data[0][0];
    const t1 = Math.max(data[data.length - 1][0], t0 + 1);
    const x = (t) => pad + ((t - t0) / (t1 - t0)) * iw;
    const y = (v) => pad + ih - ((Math.min(v, max) - min) / (max - min)) * ih;

    // Lưới ngang nhẹ (mục 55: card gọn, không dashboard loè loẹt)
    ctx.strokeStyle = grid;
    ctx.lineWidth = 1;
    for (let i = 1; i <= 3; i++) {
      const gy = pad + (ih * i) / 4;
      ctx.beginPath();
      ctx.moveTo(pad, gy);
      ctx.lineTo(pad + iw, gy);
      ctx.stroke();
    }

    // Đường (UX-4: 2px round — docs/UIUX-UPGRADE.md)
    ctx.beginPath();
    for (let i = 0; i < data.length; i++) {
      const px = x(data[i][0]);
      const py = y(data[i][1]);
      if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
    }
    ctx.strokeStyle = color;
    ctx.lineWidth = opts.lineWidth ?? 2;
    ctx.lineJoin = 'round';
    ctx.lineCap = 'round';
    ctx.stroke();

    // Tô area dưới đường — gradient dọc accent -> transparent (UX-4)
    if (opts.fill !== false) {
      const grad = ctx.createLinearGradient(0, pad, 0, pad + ih);
      grad.addColorStop(0, hexToRgba(color, 0.22));
      grad.addColorStop(1, hexToRgba(color, 0.02));
      ctx.lineTo(x(data[data.length - 1][0]), pad + ih);
      ctx.lineTo(x(data[0][0]), pad + ih);
      ctx.closePath();
      ctx.fillStyle = grad;
      ctx.fill();
    }

    // Điểm cuối + glow 2 lớp (UX-4: đắt tiền hơn nhưng vẫn canvas thuần)
    const last = data[data.length - 1];
    const lx = x(last[0]);
    const ly = y(last[1]);
    ctx.beginPath();
    ctx.arc(lx, ly, 6.5, 0, Math.PI * 2);
    ctx.fillStyle = hexToRgba(color, 0.22);
    ctx.fill();
    ctx.beginPath();
    ctx.arc(lx, ly, 2.6, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
  }
}

function hexToRgba(hex, alpha) {
  const m = hex.replace('#', '');
  const full = m.length === 3 ? m.split('').map((c) => c + c).join('') : m;
  const n = parseInt(full, 16);
  if (Number.isNaN(n)) return `rgba(120, 130, 145, ${alpha})`;
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}
