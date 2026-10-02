/**
 * Icon set — SVG inline (stroke 1.75, 24×24) để đồng bộ & luôn khớp theme.
 * Bản nâng cấp: nét vẽ cân đối hơn, bo góc đồng nhất, thêm icon mới.
 * Dùng: icon('play', 18, 'nav-icon')
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

  // ---- New additions ----
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

  // ---- UIX v3 additions (cùng ngôn ngữ stroke 1.75, bo góc đồng bộ) ----
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
};

/** Trả về markup SVG cho icon; fallback = dấu chấm. */
export function icon(name, size = 18, className = '') {
  const body = PATHS[name] || PATHS.dot;
  const cls = className ? ` class="${className}"` : '';
  return (
    `<svg${cls} width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" ` +
    `stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" ` +
    `aria-hidden="true" focusable="false">${body}</svg>`
  );
}

/** Alias tiện dụng khi chèn vào template string. */
export const ic = icon;

export function hasIcon(name) {
  return Object.prototype.hasOwnProperty.call(PATHS, name);
}