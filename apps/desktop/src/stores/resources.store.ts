import { defineStore } from 'pinia'

import {
  buildProject,
  createResourceProject,
  deleteResourceProject,
  generateProject,
  getLayerState,
  getWizardInfo,
  installPack,
  listBuilds,
  listInstalledPacks,
  moveLayerPack,
  setLayerOrder,
  uninstallPack,
  updateResourceProject,
  validateProject,
} from '@/services/resourcesCommands'
import { listResourceProjects } from '@/services/assetsCommands'
import { toAntaresError } from '@/services/ipc'
import type { ResourceProject } from '@/types/assets'
import type {
  BuildEntry,
  BuildManifest,
  InstalledPack,
  LayerState,
  ResourceWizardInfo,
  ValidationResult,
} from '@/types/resources'

export type LoadPhase = 'idle' | 'loading' | 'ready' | 'error' | 'offline'

function isOfflineCode(code: string | null): boolean {
  return code === 'IPC_SESSION_STALE' || code === 'IPC_SIDECAR_NOT_FOUND'
}

interface ResourcesState {
  phase: LoadPhase
  projects: ResourceProject[]
  selectedProjectId: string | null
  wizard: ResourceWizardInfo | null
  validation: ValidationResult | null
  validating: boolean
  generating: boolean
  builds: BuildEntry[]
  lastManifest: BuildManifest | null
  building: boolean
  busyProjectId: string | null
  installedPacks: InstalledPack[]
  layer: LayerState | null
  layerLoading: boolean
  wizardOpen: boolean
  wizardCreating: boolean
  errorMessage: string | null
  errorCode: string | null
}

/**
 * B8b — Pack lifecycle store: create (wizard) → validate → build → install
 * → layer order (§33/§126–§128/§173). Phase machine §158 cho project list.
 */
export const useResourcesStore = defineStore('resources', {
  state: (): ResourcesState => ({
    phase: 'idle',
    projects: [],
    selectedProjectId: null,
    wizard: null,
    validation: null,
    validating: false,
    generating: false,
    builds: [],
    lastManifest: null,
    building: false,
    busyProjectId: null,
    installedPacks: [],
    layer: null,
    layerLoading: false,
    wizardOpen: false,
    wizardCreating: false,
    errorMessage: null,
    errorCode: null,
  }),
  getters: {
    selectedProject(state): ResourceProject | undefined {
      return state.projects.find((p) => p.id === state.selectedProjectId)
    },
    errorCount(state): number {
      return state.validation?.findings.filter((f) => f.severity === 'ERROR').length ?? 0
    },
    warningCount(state): number {
      return state.validation?.findings.filter((f) => f.severity === 'WARNING').length ?? 0
    },
  },
  actions: {
    async load(): Promise<void> {
      this.phase = 'loading'
      this.errorMessage = null
      this.errorCode = null
      try {
        this.projects = await listResourceProjects()
        this.phase = 'ready'
      } catch (err) {
        const antares = toAntaresError(err)
        this.phase = isOfflineCode(antares.code) ? 'offline' : 'error'
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    selectProject(projectId: string | null): void {
      this.selectedProjectId = projectId
      this.validation = null
      this.builds = []
      this.lastManifest = null
    },
    openWizard(): void {
      this.wizardOpen = true
      void this.loadWizard()
    },
    closeWizard(): void {
      this.wizardOpen = false
    },
    async loadWizard(): Promise<void> {
      try {
        this.wizard = await getWizardInfo()
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async createProject(params: {
      name: string
      mcVersion: string
      template: string
      description: string
    }): Promise<boolean> {
      this.wizardCreating = true
      try {
        const project = await createResourceProject(params)
        this.wizardOpen = false
        await this.load()
        this.selectProject(project.id)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.wizardCreating = false
      }
    },
    async renameProject(projectId: string, name: string): Promise<boolean> {
      this.busyProjectId = projectId
      try {
        await updateResourceProject(projectId, { name })
        await this.load()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busyProjectId = null
      }
    },
    async removeProject(projectId: string): Promise<boolean> {
      this.busyProjectId = projectId
      try {
        await deleteResourceProject(projectId)
        if (this.selectedProjectId === projectId) this.selectProject(null)
        await this.load()
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busyProjectId = null
      }
    },
    async regenerate(projectId: string): Promise<boolean> {
      this.generating = true
      try {
        await generateProject(projectId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.generating = false
      }
    },
    async runValidation(projectId: string): Promise<boolean> {
      this.validating = true
      try {
        this.validation = await validateProject(projectId)
        return this.validation.ok
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.validating = false
      }
    },
    async build(projectId: string): Promise<boolean> {
      this.building = true
      try {
        this.lastManifest = await buildProject(projectId)
        this.builds = await listBuilds(projectId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.building = false
      }
    },
    async refreshBuilds(projectId: string): Promise<void> {
      try {
        this.builds = await listBuilds(projectId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async installToInstance(projectId: string, instanceId: string, overwrite = false): Promise<boolean> {
      this.busyProjectId = projectId
      try {
        const result = await installPack({ projectId, instanceId, overwrite })
        await this.refreshBuilds(projectId)
        this.errorMessage = null
        return result.file.length > 0
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      } finally {
        this.busyProjectId = null
      }
    },
    async refreshInstalledPacks(instanceId: string): Promise<void> {
      try {
        this.installedPacks = await listInstalledPacks(instanceId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      }
    },
    async removePack(instanceId: string, filename: string): Promise<boolean> {
      try {
        await uninstallPack(instanceId, filename)
        await this.refreshInstalledPacks(instanceId)
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async loadLayer(instanceId: string): Promise<void> {
      this.layerLoading = true
      try {
        this.layer = await getLayerState(instanceId)
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
      } finally {
        this.layerLoading = false
      }
    },
    async reorderLayer(instanceId: string, order: string[]): Promise<boolean> {
      try {
        const next = await setLayerOrder(instanceId, order)
        if (this.layer) this.layer.order = next
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
    async movePack(instanceId: string, filename: string, delta: number): Promise<boolean> {
      try {
        const next = await moveLayerPack(instanceId, filename, delta)
        if (this.layer) this.layer.order = next
        return true
      } catch (err) {
        const antares = toAntaresError(err)
        this.errorMessage = antares.message
        this.errorCode = antares.code
        return false
      }
    },
  },
})
