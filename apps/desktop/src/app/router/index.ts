import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'

import DashboardPage from '@/features/dashboard/DashboardPage.vue'
import { navigationGuards } from './guards'

export const routes: RouteRecordRaw[] = [
  { path: '/', name: 'dashboard', component: DashboardPage },
  {
    path: '/play',
    name: 'play',
    component: () => import('@/features/play/PlayPage.vue'),
  },
  {
    path: '/profiles',
    name: 'profiles',
    component: () => import('@/features/profiles/ProfilesPage.vue'),
  },
  {
    path: '/mods',
    name: 'mods',
    component: () => import('@/features/mods/ModsPage.vue'),
  },
  {
    path: '/resources',
    name: 'resources',
    component: () => import('@/features/resources/ResourceStudioPage.vue'),
  },
  {
    path: '/visuals',
    name: 'visuals',
    component: () => import('@/features/visuals/VisualStudioPage.vue'),
  },
  {
    path: '/optimization',
    name: 'optimization',
    component: () => import('@/features/optimization/OptimizationPage.vue'),
  },
  {
    path: '/network',
    name: 'network',
    component: () => import('@/features/network/NetworkPage.vue'),
  },
  {
    path: '/runtime',
    name: 'runtime',
    component: () => import('@/features/runtime/RuntimePage.vue'),
  },
  {
    path: '/diagnostics',
    name: 'diagnostics',
    component: () => import('@/features/diagnostics/DiagnosticsPage.vue'),
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('@/features/settings/SettingsPage.vue'),
  },
  {
    path: '/:pathMatch(.*)*',
    name: 'not-found',
    component: () => import('@/app/pages/NotFoundPage.vue'),
  },
]

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
})

router.beforeEach(navigationGuards)
