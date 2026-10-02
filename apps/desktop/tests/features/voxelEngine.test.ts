import { describe, expect, it } from 'vitest'

import { specToDocument } from '@/features/visuals/voxelEngine'

describe('specToDocument (B9 — Three.js engine)', () => {
  it('converts valid cubes + resolves hex texture refs', () => {
    const doc = specToDocument({
      grid: 16,
      cubes: [
        {
          id: 'body',
          from: [5, 4, 5],
          to: [11, 12, 11],
          faces: { up: { texture: '#e8b23a' }, south: { texture: '#8c5a2b' } },
        },
      ],
      textures: {},
    })
    expect(doc.grid).toBe(16)
    expect(doc.cubes).toHaveLength(1)
    expect(doc.cubes[0]?.color).toBe('#e8b23a')
    expect(doc.cubes[0]?.id).toBe('body')
  })

  it('resolves texture keys through spec.textures', () => {
    const doc = specToDocument({
      grid: 16,
      cubes: [{ id: 'c1', from: [0, 0, 0], to: [4, 4, 4], faces: { up: { texture: 'wood' } } }],
      textures: { wood: '#8c5a2b' },
    })
    expect(doc.cubes[0]?.color).toBe('#8c5a2b')
  })

  it('falls back to default color for unresolvable refs', () => {
    const doc = specToDocument({
      grid: 16,
      cubes: [{ id: 'c1', from: [0, 0, 0], to: [4, 4, 4], faces: { up: { texture: 'missing' } } }],
      textures: {},
    })
    expect(doc.cubes[0]?.color).toBe('#e8b23a')
  })

  it('skips malformed cubes and bad coords', () => {
    const doc = specToDocument({
      grid: 16,
      cubes: [
        { id: 'ok', from: [0, 0, 0], to: [4, 4, 4] },
        { id: 'bad-from', from: 'x', to: [4, 4, 4] },
        { id: 'empty', from: [2, 2, 2], to: [2, 2, 2] },
        'not-an-object',
      ],
      textures: {},
    })
    expect(doc.cubes.map((c) => c.id)).toEqual(['ok'])
  })

  it('handles non-object spec', () => {
    expect(specToDocument(null)).toEqual({ grid: 16, cubes: [] })
    expect(specToDocument('x')).toEqual({ grid: 16, cubes: [] })
  })
})
