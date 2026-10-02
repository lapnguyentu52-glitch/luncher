import { defineStore } from 'pinia'

export type BootStage = 'splash' | 'ready'

interface SessionState {
  bootStage: BootStage
  currentInstanceId: string | null
}

export const useSessionStore = defineStore('session', {
  state: (): SessionState => ({
    bootStage: 'splash',
    currentInstanceId: null,
  }),
  actions: {
    markReady(): void {
      this.bootStage = 'ready'
    },
    selectInstance(id: string): void {
      this.currentInstanceId = id
    },
  },
})
