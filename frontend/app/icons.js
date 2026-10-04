/**
 * Icon set PREMIUM — SVG inline 24×24, đồng bộ theme (currentColor).
 * Nâng cấp so với bản cũ:
 *  - Giữ NGUYÊN toàn bộ icon + API cũ: icon('play', 18, 'nav-icon')
 *  - Thêm biến thể "duo" (nền tô nhạt 2 lớp) → nhìn đầy đặn, cao cấp hơn
 *  - Thêm iconBadge(): ô icon gradient + bóng phát sáng kiểu trang seller / shop
 *  - Thêm bộ icon bán hàng: store, cart, wallet, crown, diamond, fire, trending...
 *
 * Dùng:
 *   icon('play', 18, 'nav-icon')                 // line cổ điển
 *   icon('store', 22, '', { variant: 'duo' })    // duotone
 *   iconBadge('crown', 44, 'amber')              // ô gradient nổi bật
 */
const PATHS = {
  // ---- Navigation ----
  dashboard:
    '<rect x="3" y="3" width="7.6" height="9.2" rx="2.1"/><rect x="13.4" y="3" width="7.6" height="5.6" rx="2.1"/>' +
    '<rect x="13.4" y="11.8" width="7.6" height="9.2" rx="2.1"/><rect x="3" y="15.4" width="7.6" height="5.6" rx="2.1"/>',
  play: '<path d="M7.2 4.6v14.8a1 1 0 0 0 1.53.85l11.6-7.4a1 1 0 0 0 0-1.7L8.73 3.75a1 1 0 0 0-1.53.85Z"/>',
  instances:
    '<path d="M12 2.6 3.4 7.1v9.8L12 21.4l8.6-4.5V7.1Z"/><path d="M3.4 7.1 12 11.6l8.6-4.5"/><path d="M12 11.6v9.8"/>',
  accounts:
    '<circle cx="12" cy="8.1" r="3.7"/><path d="M4.4 20.5a7.7 7.7 0 0 1 15.2 0"/>',
  mods:
    '<path d="M10.6 3h-5A1.6 1.6 0 0 0 4 4.6v5a1.6 1.6 0 0 0 .47 1.13l8.7 8.7a1.6 1.6 0 0 0 2.26 0l4.1-4.1a1.6 1.6 0 0 0 0-2.26l-8.7-8.7A1.6 1.6 0 0 0 10.6 3Z"/><circle cx="8.1" cy="7.7" r="1.35"/>',
  servers:
    '<rect x="3" y="3.4" width="18" height="7.1" rx="2.1"/><rect x="3" y="13.5" width="18" height="7.1" rx="2.1"/>' +
    '<path d="M6.6 6.95h.01M6.6 17.05h.01"/><path d="M10.3 6.95h4M10.3 17.05h4"/>',
  security:
    '<path d="M12 21.6s7.7-3.5 7.7-9.5V5.4L12 2.4 4.3 5.4v6.7c0 6 7.7 9.5 7.7 9.5Z"/><path d="m8.6 11.9 2.4 2.4 4.5-4.6"/>',
  downloads:
    '<path d="M12 3.3v11.4"/><path d="m7.4 10.3 4.6 4.6 4.6-4.6"/><path d="M4.3 19.7h15.4"/>',
  logs:
    '<path d="M14.2 3.3H7a2 2 0 0 0-2 2v13.4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.3Z"/><path d="M14.2 3.3v5h5"/><path d="M8.3 12.6h7.4M8.3 16.2h4.6"/>',
  diagnostics:
    '<path d="M3.3 12h3.3l2.4-6.4 3.7 12.8 2.5-6.4h5.5"/>',
  settings:
    '<circle cx="12" cy="12" r="3.3"/><path d="M19.5 14.7a1.65 1.65 0 0 0 .33 1.82l.06.06a1.95 1.95 0 1 1-2.76 2.76l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.5v.17a1.95 1.95 0 0 1-3.9 0v-.09a1.65 1.65 0 0 0-1.08-1.5 1.65 1.65 0 0 0-1.82.33l-.06.06a1.95 1.95 0 1 1-2.76-2.76l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.5-1h-.17a1.95 1.95 0 0 1 0-3.9h.09a1.65 1.65 0 0 0 1.5-1.08 1.65 1.65 0 0 0-.33-1.82l-.06-.06a1.95 1.95 0 1 1 2.76-2.76l.06.06a1.65 1.65 0 0 0 1.82.33h.08a1.65 1.65 0 0 0 1-1.5v-.17a1.95 1.95 0 0 1 3.9 0v.09a1.65 1.65 0 0 0 1 1.5 1.65 1.65 0 0 0 1.82-.33l.06-.06a1.95 1.95 0 1 1 2.76 2.76l-.06.06a1.65 1.65 0 0 0-.33 1.82v.08a1.65 1.65 0 0 0 1.5 1h.17a1.95 1.95 0 0 1 0 3.9h-.09a1.65 1.65 0 0 0-1.5 1Z"/>',

  // ---- Chrome / actions ----
  panelLeft:
    '<rect x="3" y="4" width="18" height="16" rx="2.6"/><path d="M9.4 4v16"/>',
  search: '<circle cx="10.9" cy="10.9" r="6.7"/><path d="m20.5 20.5-4.7-4.7"/>',
  tasks: '<path d="M4 7h16M4 12h16M4 17h9.5"/><circle cx="19.5" cy="17" r="1.3" fill="currentColor" stroke="none"/>',
  wifi: '<path d="M4 9.5a12 12 0 0 1 16 0"/><path d="M7 12.9a8 8 0 0 1 10 0"/><path d="M10.1 16.3a4 4 0 0 1 3.8 0"/><circle cx="12" cy="19.5" r=".95" fill="currentColor" stroke="none"/>',
  user: '<circle cx="12" cy="8.1" r="3.7"/><path d="M4.4 20.5a7.7 7.7 0 0 1 15.2 0"/>',
  star: '<path fill="currentColor" stroke="none" d="m12 2.4 2.97 6.02 6.63.97-4.8 4.68 1.13 6.6L12 17.35l-5.93 3.32 1.13-6.6-4.8-4.68 6.63-.97Z"/>',
  command:
    '<path d="M18 3a3 3 0 0 0-3 3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3V6a3 3 0 1 0-3 3h12a3 3 0 0 0 0-6Z"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  close: '<path d="m6 6 12 12M18 6 6 18"/>',
  eraser: '<path d="m7.5 20.2-3.7-3.7a1.7 1.7 0 0 1 0-2.4L13.4 4.5a1.7 1.7 0 0 1 2.4 0l3.7 3.7a1.7 1.7 0 0 1 0 2.4L10 20.2Z"/><path d="m8.6 9.3 6.1 6.1"/><path d="M5 20.2h16"/>',
  trash: '<path d="M4.3 7h15.4"/><path d="M9.4 7V5.4A1.6 1.6 0 0 1 11 3.8h2a1.6 1.6 0 0 1 1.6 1.6V7"/>' +
    '<path d="M6.3 7 7.2 19.3a1.7 1.7 0 0 0 1.7 1.6h6.2a1.7 1.7 0 0 0 1.7-1.6L17.7 7"/><path d="M10.3 11v6.2M13.7 11v6.2"/>',
  refresh: '<path d="M20.6 12a8.6 8.6 0 1 1-2.63-6.17"/><path d="M20.6 4.3v5.9h-5.9"/>',
  upload: '<path d="M12 20.6V9.1"/><path d="m7.4 13.5 4.6-4.6 4.6 4.6"/><path d="M4.3 5.4h15.4"/>',
  shieldCheck: '<path d="M12 21.6s7.7-3.5 7.7-9.5V5.4L12 2.4 4.3 5.4v6.7c0 6 7.7 9.5 7.7 9.5Z"/><path d="m8.6 11.9 2.4 2.4 4.5-4.6"/>',
  cpu: '<rect x="6.4" y="6.4" width="11.2" height="11.2" rx="2.4"/><rect x="9.7" y="9.7" width="4.6" height="4.6" rx="1"/><path d="M10 2.6v3.8M14 2.6v3.8M10 17.6v3.8M14 17.6v3.8M2.6 10h3.8M2.6 14h3.8M17.6 10h3.8M17.6 14h3.8"/>',
  memory: '<rect x="2.6" y="7" width="18.8" height="10" rx="2.4"/><path d="M6.8 17v3.2M12 17v3.2M17.2 17v3.2"/><path d="M6.8 11.4h10.4"/>',
  folder: '<path d="M3.3 7a2.1 2.1 0 0 1 2.1-2.1h3.4l2.1 2.5h8a2.1 2.1 0 0 1 2.1 2.1v7.4a2.1 2.1 0 0 1-2.1 2.1H5.4a2.1 2.1 0 0 1-2.1-2.1Z"/>',
  clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.3V12l3.1 2.1"/>',
  sparkle: '<path d="M12 3.2 14 9l5.8 2-5.8 2-2 5.8-2-5.8-5.8-2L10 9Z"/><path d="M18.7 3.2v3.1M20.2 4.75h-3.1"/>',
  check: '<path d="m4.6 12.6 4.8 4.8 10.2-10.3"/>',
  alert: '<circle cx="12" cy="12" r="8.7"/><path d="M12 7.6v5.2M12 16.3h.01"/>',
  info: '<circle cx="12" cy="12" r="8.7"/><path d="M12 11v5.1M12 7.7h.01"/>',
  rocket: '<path d="M12 2.6c3.4 2.1 5.3 5.7 5.3 9.7l-2.3 2.3H9l-2.3-2.3c0-4 1.9-7.6 5.3-9.7Z"/><path d="M9 14.6 7.3 18.7l3.2-1.3M15 14.6l1.7 4.1-3.2-1.3"/><circle cx="12" cy="9.8" r="1.6"/>',
  globe: '<circle cx="12" cy="12" r="8.7"/><path d="M3.3 12h17.4"/><path d="M12 3.3a13.2 13.2 0 0 1 0 17.4 13.2 13.2 0 0 1 0-17.4Z"/>',
  dot: '<circle cx="12" cy="12" r="4" fill="currentColor" stroke="none"/>',
  chevronDown: '<path d="m6 9.2 6 6.2 6-6.2"/>',
  layers: '<path d="m12 2.8 8.7 4.5L12 11.8 3.3 7.3Z"/><path d="m3.3 12.4 8.7 4.5 8.7-4.5"/><path d="m3.3 17 8.7 4.5 8.7-4.5"/>',

  // ---- Additions ----
  power: '<path d="M12 3.2v8"/><path d="M7.2 6.4a8 8 0 1 0 9.6 0"/>',
  bell: '<path d="M6 10.2a6 6 0 0 1 12 0c0 4.4 1.6 5.7 1.6 5.7H4.4S6 14.6 6 10.2Z"/><path d="M10.3 19.4a1.9 1.9 0 0 0 3.4 0"/>',
  database: '<ellipse cx="12" cy="6" rx="7.6" ry="3"/><path d="M4.4 6v6c0 1.66 3.4 3 7.6 3s7.6-1.34 7.6-3V6"/><path d="M4.4 12v6c0 1.66 3.4 3 7.6 3s7.6-1.34 7.6-3v-6"/>',
  edit: '<path d="M4 20h4.2L18.8 9.4a2.3 2.3 0 0 0-3.2-3.2L5.2 16.8Z"/><path d="m13.6 7.6 2.8 2.8"/>',
  copy: '<rect x="9" y="9" width="11.4" height="11.4" rx="2.1"/><path d="M15 9V5.6A2.1 2.1 0 0 0 12.9 3.5H5.6A2.1 2.1 0 0 0 3.5 5.6v7.3A2.1 2.1 0 0 0 5.6 15H9"/>',
  externalLink: '<path d="M9.5 5.4H5.6A2.1 2.1 0 0 0 3.5 7.5v10.9a2.1 2.1 0 0 0 2.1 2.1h10.9a2.1 2.1 0 0 0 2.1-2.1v-3.9"/><path d="M14.6 3.4h6v6"/><path d="M20.4 3.6 11.8 12.2"/>',
  arrowRight: '<path d="M4 12h16"/><path d="m14 6 6 6-6 6"/>',
  arrowLeft: '<path d="M20 12H4"/><path d="m10 6-6 6 6 6"/>',
  cube: '<path d="m12 2.8 8.7 4.5v9.4L12 21.2l-8.7-4.5V7.3Z"/><path d="m3.3 7.3 8.7 4.5 8.7-4.5"/><path d="M12 11.8v9.4"/>',
  eye: '<path d="M2.5 12S6 5.8 12 5.8 21.5 12 21.5 12 18 18.2 12 18.2 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="2.9"/>',
  link: '<path d="M9.4 13.8a4.4 4.4 0 0 0 6.2.4l3-3a4.4 4.4 0 1 0-6.2-6.2l-1.5 1.5"/><path d="M14.6 10.2a4.4 4.4 0 0 0-6.2-.4l-3 3a4.4 4.4 0 1 0 6.2 6.2l1.5-1.5"/>',
  moon: '<path d="M20.2 14.3a8.7 8.7 0 1 1-10.5-10.5 7 7 0 0 0 10.5 10.5Z"/>',
  sun: '<circle cx="12" cy="12" r="4.4"/><path d="M12 2.6v2.4M12 19v2.4M4.6 12H2.2M21.8 12h-2.4M5.4 5.4l1.7 1.7M17 17l1.7 1.7M18.6 5.4 16.9 7M7 17l-1.7 1.7"/>',

  // ---- UIX v3 ----
  users: '<circle cx="9" cy="8.2" r="3.4"/><path d="M2.9 19.8a6.2 6.2 0 0 1 12.2 0"/><path d="M16.4 5.2a3.4 3.4 0 0 1 0 6"/><path d="M17.8 14.4a6.2 6.2 0 0 1 3.4 5.4"/>',
  image: '<rect x="3.2" y="4.2" width="17.6" height="15.6" rx="2.4"/><circle cx="9" cy="9.6" r="1.7"/><path d="m3.6 17.2 4.9-4.8a1.6 1.6 0 0 1 2.26 0l2.44 2.44"/><path d="m14.4 13.6 1.9-1.9a1.6 1.6 0 0 1 2.26 0l1.84 1.84"/>',
  save: '<path d="M5.4 3.5h10.2L20.5 8.4v10.1a1.9 1.9 0 0 1-1.9 1.9H5.4a1.9 1.9 0 0 1-1.9-1.9V5.4a1.9 1.9 0 0 1 1.9-1.9Z"/><path d="M8 3.6v5h7v-5"/><rect x="7.6" y="13" width="8.8" height="7.2" rx="0.8"/>',
  download: '<path d="M12 3.4v11.2"/><path d="m7.5 10.2 4.5 4.5 4.5-4.5"/><path d="M4.4 20.4h15.2"/>',
  filter: '<path d="M4 5.2h16l-6.2 7.3v6.2l-3.6-1.9v-4.3Z"/>',
  sliders: '<path d="M5 4.4v5.2M5 14.4v5.2M12 4.4v2.6M12 11.8v7.8M19 4.4v7.2M19 16.4v3.2"/><circle cx="5" cy="12" r="2.2"/><circle cx="12" cy="9.4" r="2.2"/><circle cx="19" cy="14" r="2.2"/>',
  monitor: '<rect x="2.8" y="4" width="18.4" height="12.6" rx="2.2"/><path d="M9 20.6h6M12 16.8v3.6"/>',
  zap: '<path d="M13.2 2.6 4.8 13.4h6l-1 8 8.4-10.8h-6Z"/>',
  tag: '<path d="m3.6 11.2 7.6-7.6h7.2v7.2l-7.6 7.6a1.8 1.8 0 0 1-2.55 0L3.6 13.75a1.8 1.8 0 0 1 0-2.55Z"/><circle cx="15.1" cy="8.9" r="1.4"/>',
  book: '<path d="M4.4 19.2V5.6a2 2 0 0 1 2-2h13.2v13.6"/><path d="M6.4 17.2h13.2v3.8H6.4a1.9 1.9 0 0 1 0-3.8Z"/>',
  xCircle: '<circle cx="12" cy="12" r="8.7"/><path d="m9.2 9.2 5.6 5.6M14.8 9.2l-5.6 5.6"/>',
  pause: '<rect x="6.4" y="4.6" width="3.6" height="14.8" rx="1.2"/><rect x="14" y="4.6" width="3.6" height="14.8" rx="1.2"/>',
  shieldAlert: '<path d="M12 21.6s7.7-3.5 7.7-9.5V5.4L12 2.4 4.3 5.4v6.7c0 6 7.7 9.5 7.7 9.5Z"/><path d="M12 8.2v4.4M12 15.9h.01"/>',
  heart: '<path d="M12 20.4S3.6 15.5 3.6 9.3a4.6 4.6 0 0 1 8.4-2.6A4.6 4.6 0 0 1 20.4 9.3c0 6.2-8.4 11.1-8.4 11.1Z"/>',
  activity: '<path d="M3 12h3.4l2.3-6.1 3.6 12.4 2.4-6.3H21"/>',
  brush: '<path d="M9.4 20.6c-1.7 0-3-1.2-3-2.9 0-1 .5-1.6 1.2-2.2 1.6-1.4.5-3.5-1.2-3.5A4.3 4.3 0 0 1 2 7.7C2 5 4.4 3 7.5 3c4.9 0 8.7 3.8 8.7 8.7 0 4.9-3.2 8.9-6.8 8.9Z"/><path d="M13.2 13.4 20.8 5.8a1.4 1.4 0 0 1 2 2l-7.6 7.6"/>',

  // ---- SELLER / SHOP pack (mới) ----
  store:
    '<path d="M4 9.5 5.4 4.6a1.5 1.5 0 0 1 1.45-1.1h10.3a1.5 1.5 0 0 1 1.45 1.1L20 9.5"/>' +
    '<path d="M4 9.5a2.67 2.67 0 0 0 5.33 0 2.67 2.67 0 0 0 5.34 0A2.67 2.67 0 0 0 20 9.5"/>' +
    '<path d="M5.2 12.4V19a1.5 1.5 0 0 0 1.5 1.5h10.6a1.5 1.5 0 0 0 1.5-1.5v-6.6"/>' +
    '<path d="M9.8 20.5v-4.3a1 1 0 0 1 1-1h2.4a1 1 0 0 1 1 1v4.3"/>',
  cart: '<circle cx="9.2" cy="19.8" r="1.4"/><circle cx="17.2" cy="19.8" r="1.4"/><path d="M2.8 3.6h2.6l2.1 11.1a1.6 1.6 0 0 0 1.6 1.3h7.6a1.6 1.6 0 0 0 1.55-1.2L20 8H6.2"/>',
  bag: '<path d="M5.2 8h13.6l.9 11a1.8 1.8 0 0 1-1.8 2H6.1a1.8 1.8 0 0 1-1.8-2Z"/><path d="M8.6 10.2V7a3.4 3.4 0 0 1 6.8 0v3.2"/>',
  wallet: '<path d="M19.5 8V6.2a2 2 0 0 0-2-2H5.6a2.4 2.4 0 0 0-2.4 2.4v10.8a2.4 2.4 0 0 0 2.4 2.4h12.9a2 2 0 0 0 2-2V10a2 2 0 0 0-2-2H5.6a2.4 2.4 0 0 1-2.4-1.4"/><circle cx="16.4" cy="14" r="1.25" fill="currentColor" stroke="none"/>',
  creditCard: '<rect x="3" y="5" width="18" height="14" rx="2.6"/><path d="M3 10h18"/><path d="M7 15h3.2"/>',
  percent: '<path d="M19 5 5 19"/><circle cx="7.2" cy="7.2" r="2.4"/><circle cx="16.8" cy="16.8" r="2.4"/>',
  gift: '<rect x="3.6" y="8.4" width="16.8" height="4" rx="1.4"/><path d="M5 12.4V19a1.6 1.6 0 0 0 1.6 1.6h10.8A1.6 1.6 0 0 0 19 19v-6.6"/><path d="M12 8.4v12.2"/><path d="M12 8.4C10.4 8.4 7.4 8 7.4 5.9a2 2 0 0 1 3.6-1.2c.6.8 1 2.2 1 3.7Z"/><path d="M12 8.4c1.6 0 4.6-.4 4.6-2.5a2 2 0 0 0-3.6-1.2c-.6.8-1 2.2-1 3.7Z"/>',
  crown: '<path d="m3.4 8 4.6 4.2L12 5l4 7.2L20.6 8l-1.7 10.6H5.1Z"/><path d="M5.6 21h12.8"/>',
  diamond: '<path d="M6.8 3.8h10.4l3.6 5L12 20.4 3.2 8.8Z"/><path d="M3.2 8.8h17.6"/><path d="m9.2 3.8-1.6 5L12 20.4l4.4-11.6-1.6-5"/>',
  fire: '<path d="M12 21.4c3.9 0 6.6-2.6 6.6-6.2 0-2.5-1.3-4.3-2.7-5.9-.3 1.4-1 2.3-2 2.8.4-3.4-.8-6.4-3.6-8.6.1 3.3-1.8 4.9-3.3 6.8-1 1.3-1.6 2.6-1.6 4.4 0 3.5 2.7 6.7 6.6 6.7Z"/><path d="M12 21.4c-1.7 0-2.9-1.2-2.9-2.8 0-1.5 1.2-2.4 2-3.6.9 1 3.8 2 3.8 4.2 0 1.3-1.1 2.2-2.9 2.2Z"/>',
  trending: '<path d="m3.4 16.6 5.6-5.8 3.6 3.6 7.8-8"/><path d="M15 6.4h5.4v5.4"/>',
  truck: '<rect x="2.8" y="5.4" width="10.4" height="11" rx="2"/><path d="M13.2 9h4.2l3.4 3.8v3.6h-7.6"/><circle cx="7" cy="18.4" r="1.8"/><circle cx="17" cy="18.4" r="1.8"/>',
  badgeCheck: '<path d="m12 2.8 2.4 1.9 3-.2.9 2.9 2.5 1.7-1 2.8 1 2.8-2.5 1.7-.9 2.9-3-.2L12 21.2l-2.4-1.9-3 .2-.9-2.9-2.5-1.7 1-2.8-1-2.8 2.5-1.7.9-2.9 3 .2Z"/><path d="m8.8 12.2 2.3 2.3 4.1-4.4"/>',
  receipt: '<path d="M5.4 3.4h13.2v17.2l-2.2-1.5-2.2 1.5-2.2-1.5-2.2 1.5-2.2-1.5-2.2 1.5Z"/><path d="M8.6 8.2h6.8M8.6 12h6.8M8.6 15.6h3.6"/>',
  headset: '<path d="M4 14v-2a8 8 0 0 1 16 0v2"/><rect x="3" y="13.4" width="4.2" height="6" rx="1.8"/><rect x="16.8" y="13.4" width="4.2" height="6" rx="1.8"/><path d="M19 19.4c0 1.3-1.6 2-4 2h-2"/>',
  chat: '<path d="M20.6 11.6a7.9 7.9 0 0 1-11.5 7L3.6 20.4l1.7-5A7.9 7.9 0 1 1 20.6 11.6Z"/><path d="M8.4 10.6h7.2M8.4 13.8h4.2"/>',
  package: '<path d="m12 2.8 8.7 4.5v9.4L12 21.2l-8.7-4.5V7.3Z"/><path d="m3.3 7.3 8.7 4.5 8.7-4.5"/><path d="M12 11.8v9.4"/><path d="m7.6 5 8.7 4.5"/>',
  award: '<circle cx="12" cy="9.2" r="5.6"/><path d="m8.6 14-1.4 7 4.8-2.6 4.8 2.6-1.4-7"/>',
  coins: '<ellipse cx="9.6" cy="7" rx="6" ry="2.8"/><path d="M3.6 7v5c0 1.5 2.7 2.8 6 2.8s6-1.3 6-2.8V7"/><path d="M15.6 10.2c3 .2 5.2 1.3 5.2 2.8v4.2c0 1.5-2.7 2.8-6 2.8-2.6 0-4.8-.8-5.6-1.9"/>',
  lock: '<rect x="4.6" y="10.4" width="14.8" height="10.2" rx="2.4"/><path d="M8 10.4V7.6a4 4 0 0 1 8 0v2.8"/><path d="M12 14.6v2.4"/>',
  key: '<circle cx="8" cy="15.6" r="4.4"/><path d="m11.2 12.4 8-8M16.4 7.2l2.4 2.4M14 9.6l2 2"/>',
  calendar: '<rect x="3.4" y="4.8" width="17.2" height="15.8" rx="2.6"/><path d="M3.4 9.6h17.2M8 2.8v3.6M16 2.8v3.6"/>',
  mail: '<rect x="3" y="5" width="18" height="14" rx="2.6"/><path d="m3.6 7.2 8.4 6.2 8.4-6.2"/>',
  barChart: '<path d="M4 20.4h16"/><rect x="5.4" y="11" width="3.4" height="7.2" rx="1"/><rect x="10.3" y="5.4" width="3.4" height="12.8" rx="1"/><rect x="15.2" y="8.6" width="3.4" height="9.6" rx="1"/>',
  pieChart: '<path d="M12 3.4v8.6h8.6A8.6 8.6 0 0 0 12 3.4Z"/><path d="M20.2 14.4A8.6 8.6 0 1 1 9.6 3.8"/>',
};

/* ------------------------------------------------------------------ */
/*  Duotone: tự động tô nhạt các hình khép kín → icon đầy đặn, sang hơn */
/* ------------------------------------------------------------------ */
const DUO_FILL = 'fill="currentColor" fill-opacity=".16"';

function toDuo(body) {
  return body
    .replace(/<(rect|circle|ellipse)(?![^>]*\sfill=)/g, `<$1 ${DUO_FILL}`)
    .replace(/<path(?![^>]*\sfill=)([^>]*\sd="[^"]*[Zz]")/g, `<path ${DUO_FILL}$1`);
}

/**
 * Trả về markup SVG cho icon; fallback = dấu chấm.
 * @param {string} name
 * @param {number} size
 * @param {string} className
 * @param {{variant?: 'line'|'duo', stroke?: number}} [opts]
 */
export function icon(name, size = 18, className = '', opts = {}) {
  const { variant = 'line', stroke = 1.75 } = opts;
  let body = PATHS[name] || PATHS.dot;
  if (variant === 'duo') body = toDuo(body);
  const cls = className ? ` class="${className}"` : '';
  return (
    `<svg${cls} width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" ` +
    `stroke="currentColor" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round" ` +
    `aria-hidden="true" focusable="false">${body}</svg>`
  );
}

/** Alias tiện dụng khi chèn vào template string. */
export const ic = icon;

export function hasIcon(name) {
  return Object.prototype.hasOwnProperty.call(PATHS, name);
}

export function iconNames() {
  return Object.keys(PATHS);
}

/* ------------------------------------------------------------------ */
/*  Badge gradient + glow — kiểu trang seller / marketplace            */
/* ------------------------------------------------------------------ */
export const TONES = {
  violet:  ['#8b5cf6', '#6d28d9', 'rgba(124,58,237,.45)'],
  blue:    ['#38bdf8', '#2563eb', 'rgba(37,99,235,.45)'],
  emerald: ['#34d399', '#059669', 'rgba(5,150,105,.45)'],
  amber:   ['#fbbf24', '#d97706', 'rgba(217,119,6,.45)'],
  orange:  ['#fb923c', '#ea580c', 'rgba(234,88,12,.45)'],
  rose:    ['#fb7185', '#e11d48', 'rgba(225,29,72,.45)'],
  pink:    ['#f472b6', '#c026d3', 'rgba(192,38,211,.45)'],
  slate:   ['#64748b', '#1e293b', 'rgba(30,41,59,.45)'],
};

/**
 * Ô icon gradient bo góc, viền sáng + bóng phát sáng.
 * Dùng: iconBadge('crown', 44, 'amber')
 * @param {string} name
 * @param {number} size  kích thước ô (px)
 * @param {keyof typeof TONES} tone
 * @param {{shape?: 'squircle'|'circle', variant?: 'line'|'duo'}} [opts]
 */
export function iconBadge(name, size = 40, tone = 'violet', opts = {}) {
  const { shape = 'squircle', variant = 'duo' } = opts;
  const [c1, c2, glow] = TONES[tone] || TONES.violet;
  const radius = shape === 'circle' ? '50%' : `${Math.round(size * 0.3)}px`;
  const style =
    `display:inline-flex;align-items:center;justify-content:center;flex:none;` +
    `width:${size}px;height:${size}px;border-radius:${radius};color:#fff;` +
    `background:linear-gradient(145deg,${c1} 0%,${c2} 100%);` +
    `box-shadow:0 8px 20px -6px ${glow},0 2px 6px rgba(0,0,0,.18),` +
    `inset 0 1px 0 rgba(255,255,255,.45),inset 0 -2px 6px rgba(0,0,0,.14);`;
  return `<span class="icon-badge icon-badge--${tone}" style="${style}">` +
    `${icon(name, Math.round(size * 0.52), '', { variant, stroke: 1.8 })}</span>`;
}