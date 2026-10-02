/**
 * Voxel 3D engine — domain-independent (§129/§130/§131).
 *
 * Pipeline §129: Document → Scene graph → Renderer → (Minecraft exporter).
 * Tầng Vue không tự hiểu format Minecraft JSON — mọi thứ đi qua engine này.
 *
 * Memory management §131 là BẮT BUỘC: dispose() phải được gọi khi rời tab
 * (pause render loop + remove listeners + dispose GPU resources).
 */

import type * as ThreeNS from 'three'

/** §129 — ModelDocument: geometry thuần, không chứa khái niệm Minecraft. */
export interface VoxelDocument {
  /** Kích thước grid tham chiếu (16 = chuẩn item model MC). */
  grid: number
  /** Mỗi cube: id + bounds [min, max] theo 3 trục, trong [-16, 32]. */
  cubes: Array<{
    id: string
    from: [number, number, number]
    to: [number, number, number]
    color: string
  }>
}

export type RenderQuality = 'low' | 'medium' | 'high'

/** §130 — debug/developer modes (editor mode). */
export interface DebugOptions {
  wireframe: boolean
  bounds: boolean
  axis: boolean
}

export interface VoxelEngineOptions {
  canvas: HTMLCanvasElement
  /** §130 — render quality. */
  quality?: RenderQuality
}

const QUALITY_SETTINGS: Record<RenderQuality, { antialias: boolean; pixelRatio: number }> = {
  low: { antialias: false, pixelRatio: 1 },
  medium: { antialias: true, pixelRatio: 1.5 },
  high: { antialias: true, pixelRatio: 2 },
}

function clampHex(color: string): string {
  return /^#[0-9a-fA-F]{6}$/.test(color) ? color : '#e8b23a'
}

/**
 * VoxelEngine — owns scene, camera, renderer, render loop.
 * Dùng async factory `VoxelEngine.create()` — three.js là dynamic import để
 * chunk 3D chỉ tải khi cần (§152/§153).
 * dispose() phải được gọi khi rời tab (§131).
 */
export class VoxelEngine {
  private readonly renderer: ThreeNS.WebGLRenderer
  private readonly scene: ThreeNS.Scene
  private readonly camera: ThreeNS.PerspectiveCamera
  private readonly root: ThreeNS.Group
  private readonly THREE: typeof ThreeNS
  private readonly disposables: Array<{ dispose(): void }> = []
  private rafId = 0
  private disposed = false

  private constructor(THREE: typeof ThreeNS, options: VoxelEngineOptions) {
    this.THREE = THREE
    const quality = QUALITY_SETTINGS[options.quality ?? 'medium']
    this.renderer = new THREE.WebGLRenderer({
      canvas: options.canvas,
      antialias: quality.antialias,
      alpha: true,
    })
    this.renderer.setPixelRatio(Math.min(quality.pixelRatio, window.devicePixelRatio || 1))

    this.scene = new THREE.Scene()
    this.camera = new THREE.PerspectiveCamera(40, 1, 0.1, 1000)
    this.camera.position.set(28, 22, 28)
    this.camera.lookAt(0, 0, 0)

    const ambient = new THREE.AmbientLight(0xffffff, 0.85)
    const dir = new THREE.DirectionalLight(0xffffff, 1.4)
    dir.position.set(1, 2, 1.4)
    this.scene.add(ambient, dir)
    this.disposables.push(ambient, dir)

    this.root = new THREE.Group()
    this.scene.add(this.root)
  }

  /** Async factory — dynamic import three (chunk riêng, load-on-demand). */
  static async create(options: VoxelEngineOptions): Promise<VoxelEngine> {
    const THREE = await import('three')
    return new VoxelEngine(THREE, options)
  }

  /** §129 — Document → Scene graph (rebuild thay vì mutate để tránh leak). */
  setDocument(doc: VoxelDocument, debug?: DebugOptions): void {
    if (this.disposed) return
    this.clearRoot()
    const wireframe = debug?.wireframe ?? false
    const showBounds = debug?.bounds ?? false
    const showAxis = debug?.axis ?? false

    if (showAxis) {
      const axes = new this.THREE.AxesHelper(18)
      this.root.add(axes)
      this.disposables.push(axes)
    }

    const boundsMaterial = new this.THREE.LineBasicMaterial({ color: 0x7fd4ff })
    const boxGeometry = new this.THREE.BoxGeometry(1, 1, 1)

    for (const cube of doc.cubes) {
      const sx = Math.abs(cube.to[0] - cube.from[0])
      const sy = Math.abs(cube.to[1] - cube.from[1])
      const sz = Math.abs(cube.to[2] - cube.from[2])
      if (sx <= 0 || sy <= 0 || sz <= 0) continue

      const color = new this.THREE.Color(clampHex(cube.color))
      const material = new this.THREE.MeshLambertMaterial({
        color,
        wireframe,
        transparent: true,
        opacity: 0.92,
      })
      const mesh = new this.THREE.Mesh(boxGeometry, material)
      mesh.scale.set(sx, sy, sz)
      mesh.position.set(
        (cube.from[0] + cube.to[0]) / 2 - doc.grid / 2,
        (cube.from[1] + cube.to[1]) / 2 - doc.grid / 2,
        (cube.from[2] + cube.to[2]) / 2 - doc.grid / 2,
      )
      this.root.add(mesh)
      this.disposables.push(material)

      if (showBounds) {
        const edges = new this.THREE.LineSegments(
          new this.THREE.EdgesGeometry(mesh.geometry),
          boundsMaterial,
        )
        edges.scale.copy(mesh.scale)
        edges.position.copy(mesh.position)
        this.root.add(edges)
        this.disposables.push(edges.geometry)
      }
    }
    boundsMaterial.dispose()
    boxGeometry.dispose()
  }

  /** §130 — quality đổi runtime: setPixelRatio (renderer giữ nguyên canvas). */
  setQuality(quality: RenderQuality): void {
    const q = QUALITY_SETTINGS[quality]
    this.renderer.setPixelRatio(Math.min(q.pixelRatio, window.devicePixelRatio || 1))
    this.render()
  }

  resize(width: number, height: number): void {
    if (this.disposed) return
    this.renderer.setSize(width, height, false)
    this.camera.aspect = width / Math.max(1, height)
    this.camera.updateProjectionMatrix()
    this.render()
  }

  render(): void {
    if (this.disposed) return
    this.renderer.render(this.scene, this.camera)
  }

  /** Continuous loop — pause bằng cancelLoop() khi rời tab (§131). */
  startLoop(): void {
    if (this.disposed || this.rafId) return
    const tick = (): void => {
      if (this.disposed) return
      this.root.rotation.y += 0.004
      this.renderer.render(this.scene, this.camera)
      this.rafId = requestAnimationFrame(tick)
    }
    this.rafId = requestAnimationFrame(tick)
  }

  cancelLoop(): void {
    if (this.rafId) {
      cancelAnimationFrame(this.rafId)
      this.rafId = 0
    }
  }

  private clearRoot(): void {
    this.root.clear()
  }

  /**
   * §131 — bắt buộc khi rời tab: pause loop + dispose toàn bộ GPU resources
   * (geometry/material/texture/renderTarget). Sau dispose() engine không dùng lại được.
   */
  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.cancelLoop()
    this.root.clear()
    for (const d of this.disposables) d.dispose()
    this.disposables.length = 0
    this.renderer.dispose()
  }
}

/**
 * VoxelSpec (legacy Minecraft JSON-ish) → VoxelDocument.
 * Đầu chỉ nhận raw spec từ bridge; resolve texture ref (hex | key | asset) và
 * lọc cube lỗi — tầng Vue làm việc chỉ với VoxelDocument.
 */
export function specToDocument(spec: unknown): VoxelDocument {
  if (typeof spec !== 'object' || spec === null) return { grid: 16, cubes: [] }
  const s = spec as Record<string, unknown>
  const grid = typeof s.grid === 'number' && s.grid > 0 ? s.grid : 16
  const textures = (typeof s.textures === 'object' && s.textures !== null)
    ? (s.textures as Record<string, unknown>)
    : {}

  const resolveColor = (value: unknown): string | null => {
    if (typeof value !== 'string' || !value.trim()) return null
    const v = value.trim()
    if (/^#[0-9a-fA-F]{3}$/.test(v) || /^#[0-9a-fA-F]{6}$/.test(v)) {
      return v.length === 4
        ? `#${v[1]}${v[1]}${v[2]}${v[2]}${v[3]}${v[3]}`
        : v
    }
    const target = textures[v]
    return resolveColor(target)
  }

  const cubes: VoxelDocument['cubes'] = []
  if (Array.isArray(s.cubes)) {
    for (const raw of s.cubes) {
      if (typeof raw !== 'object' || raw === null) continue
      const c = raw as Record<string, unknown>
      const from = c.from
      const to = c.to
      if (!Array.isArray(from) || from.length !== 3 || !Array.isArray(to) || to.length !== 3) continue
      const nums = [...from, ...to].map((v) => (typeof v === 'number' && Number.isFinite(v) ? v : NaN))
      if (nums.some((v) => Number.isNaN(v))) continue
      const [fx, fy, fz, tx, ty, tz] = nums as [number, number, number, number, number, number]
      if (tx - fx <= 0 || ty - fy <= 0 || tz - fz <= 0) continue
      const faces = (typeof c.faces === 'object' && c.faces !== null) ? (c.faces as Record<string, unknown>) : {}
      const up = (typeof faces.up === 'object' && faces.up !== null) ? (faces.up as Record<string, unknown>).texture : undefined
      const color = resolveColor(up) ?? '#e8b23a'
      cubes.push({
        id: typeof c.id === 'string' && c.id ? c.id : `cube-${cubes.length}`,
        from: [fx, fy, fz],
        to: [tx, ty, tz],
        color,
      })
    }
  }
  return { grid, cubes }
}
