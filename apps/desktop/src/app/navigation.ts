export type SidebarMode = 'expanded' | 'compact' | 'auto'

export interface NavItem {
  id: string
  label: string
  path: string
  icon: string
}

/**
 * Icons SVG path 24×24 stroke-based (feather-style) — rõ nét ở mọi size.
 * Key khớp NAV_ITEMS[].icon.
 */
export const NAV_ICONS: Record<string, string> = {
  dashboard: 'M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z',
  play: 'M8 5.5v13l11-6.5z',
  profiles: 'M12 12a4 4 0 100-8 4 4 0 000 8zM5 20a7 7 0 0114 0',
  mods: 'M21 8l-9-5-9 5v8l9 5 9-5zM3 8l9 5 9-5M12 13v8',
  resources: 'M4 7h16M4 12h16M4 17h10',
  visuals: 'M12 3l2.4 5.6L20 11l-5.6 2.4L12 19l-2.4-5.6L4 11l5.6-2.4z',
  optimization: 'M13 2L4 14h6l-1 8 9-12h-6z',
  network: 'M3 12h4l3-8 4 16 3-8h4',
  runtime: 'M2 12a10 10 0 1020 0 10 10 0 00-20 0zM12 7v5l3 3',
  diagnostics: 'M4 21V4a1 1 0 011-1h9l5 5v13a1 1 0 01-1 1H5a1 1 0 01-1-1zM14 3v5h5M8 13h8M8 17h5',
  settings:
    'M12 15a3 3 0 100-6 3 3 0 000 6z M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 11-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 11-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 11-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 110-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 112.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 114 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 112.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 110 4h-.09a1.65 1.65 0 00-1.51 1z',
}

// §6.1: dashboard, play trước; các mục còn lại sẽ lộ dần theo batch
export const NAV_ITEMS: NavItem[] = [
  { id: 'dashboard', label: 'Dashboard', path: '/', icon: 'dashboard' },
  { id: 'play', label: 'Play', path: '/play', icon: 'play' },
  { id: 'profiles', label: 'Profiles', path: '/profiles', icon: 'profiles' },
  { id: 'mods', label: 'Mods', path: '/mods', icon: 'mods' },
  { id: 'resources', label: 'Resource', path: '/resources', icon: 'resources' },
  { id: 'visuals', label: 'Visual', path: '/visuals', icon: 'visuals' },
  { id: 'optimization', label: 'Optimize', path: '/optimization', icon: 'optimization' },
  { id: 'network', label: 'Network', path: '/network', icon: 'network' },
  { id: 'runtime', label: 'Runtime', path: '/runtime', icon: 'runtime' },
  { id: 'diagnostics', label: 'Diagnostics', path: '/diagnostics', icon: 'diagnostics' },
  { id: 'settings', label: 'Settings', path: '/settings', icon: 'settings' },
]
