import { describe, expect, it } from 'vitest'

import { routes, router } from './index'

describe('router', () => {
  it('không có route resolve về null', () => {
    for (const route of routes) {
      expect(router.resolve(route.path as string)).toBeTruthy()
    }
  })

  it('catch-all not-found tồn tại', () => {
    const resolved = router.resolve('/khong-ton-tai')
    expect(resolved.name).toBe('not-found')
  })

  it('dashboard là route gốc', () => {
    const resolved = router.resolve('/')
    expect(resolved.name).toBe('dashboard')
  })
})
