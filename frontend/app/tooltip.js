/**
 * Tooltip nhẹ — CSS thuần (styles/components.css §TOOLTIP), không có thư viện
 * positioning. Thay `title` mặc định của trình duyệt (hiện chậm ~1s, không
 * theme được, khác nhau giữa các OS) bằng `data-tip` để CSS render tức thì
 * và đồng bộ theme sáng/tối + accent.
 *
 * Cách dùng: gọi applyTooltips() sau mỗi lần render DOM mới (topbar, sidebar,
 * nội dung tab...). An toàn khi gọi lặp lại — phần tử đã chuyển đổi sẽ có
 * [data-tip] nên bị bộ chọn `[title]` bỏ qua ở lần sau.
 */
export function applyTooltips(root = document) {
  if (!root || !root.querySelectorAll) return;
  const els = root.querySelectorAll('[title]');
  els.forEach((el) => {
    const label = el.getAttribute('title');
    // Input/textarea giữ nguyên title gốc (không che khuất nội dung nhập liệu).
    if (!label || el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') return;
    el.removeAttribute('title');
    if (!el.hasAttribute('data-tip')) el.setAttribute('data-tip', label);
  });
}
