/**
 * ThumbCache — LRU cache cho asset thumbnails (master plan v3 mục 79).
 *
 * Cache strategy mục 79: LRU + max entries + max estimated bytes +
 * invalidation by hash. Tránh re-fetch data URI qua bridge mỗi lần vlist
 * repaint (mục 18: search/filter không rebuild toàn DOM → cache giữ hit).
 *
 * LRU: Map giữ insertion order — get() re-insert để đẩy lên đầu; evict từ
 * đầu Map (ít dùng nhất). Bytes estimate: data URI length ≈ bytes thật.
 */

export class ThumbCache {
  /**
   * @param {object} opts
   * @param {number} [opts.maxEntries=256] số entry tối đa
   * @param {number} [opts.maxBytes=8*1024*1024] tổng bytes ước tính tối đa
   */
  constructor({ maxEntries = 256, maxBytes = 8 * 1024 * 1024 } = {}) {
    this.maxEntries = Math.max(1, maxEntries);
    this.maxBytes = Math.max(1024, maxBytes);
    this._map = new Map();       // key -> {value, bytes}
    this._bytes = 0;
    // stats (mục 78 performance budget: đo được, không đo mò)
    this.hits = 0;
    this.misses = 0;
    this.evictions = 0;
  }

  get size() { return this._map.size; }
  get bytes() { return this._bytes; }

  /**
   * Lấy từ cache. key thường là `${assetId}:${sha256.slice(0,12)}` —
   * hash đổi → cache miss tự nhiên (invalidation by hash, mục 79).
   * @returns {string|null} value hoặc null nếu miss
   */
  get(key) {
    const hit = this._map.get(key);
    if (hit === undefined) {
      this.misses++;
      return null;
    }
    // LRU touch: xoá + re-insert -> cuối Map (mới dùng nhất)
    this._map.delete(key);
    this._map.set(key, hit);
    this.hits++;
    return hit.value;
  }

  /**
   * Đặt giá trị. value nên là data URI (string) — bytes = value.length.
   * Evict LRU đến khi vừa maxEntries + maxBytes.
   */
  set(key, value) {
    if (typeof value !== 'string' || !value) return;
    const bytes = value.length;
    if (bytes > this.maxBytes) return;        // entry đơn quá lớn — không cache

    // cập nhật entry cũ nếu có
    const old = this._map.get(key);
    if (old !== undefined) {
      this._bytes -= old.bytes;
      this._map.delete(key);
    }

    this._map.set(key, { value, bytes });
    this._bytes += bytes;

    // Evict LRU (đầu Map) đến khi nằm trong cả 2 ngưỡng
    while (this._map.size > this.maxEntries ||
           (this._bytes > this.maxBytes && this._map.size > 1)) {
      const oldestKey = this._map.keys().next().value;
      const oldest = this._map.get(oldestKey);
      this._map.delete(oldestKey);
      this._bytes -= oldest.bytes;
      this.evictions++;
    }
  }

  /** Xoá 1 key (invalidation chủ động khi asset bị xoá/reassign). */
  invalidate(key) {
    const hit = this._map.get(key);
    if (hit === undefined) return false;
    this._map.delete(key);
    this._bytes -= hit.bytes;
    return true;
  }

  /** Xoá toàn bộ (logout/import lớn/theme change). */
  clear() {
    this._map.clear();
    this._bytes = 0;
  }

  /** Stats cho diagnostics (mục 78). */
  stats() {
    return {
      entries: this._map.size, bytes: this._bytes,
      hits: this.hits, misses: this.misses, evictions: this.evictions,
      hitRate: (this.hits + this.misses)
        ? +(this.hits / (this.hits + this.misses)).toFixed(3) : 0,
    };
  }
}

/** Singleton dùng chung các view — cache sống qua tab switch. */
export const thumbCache = new ThumbCache();
