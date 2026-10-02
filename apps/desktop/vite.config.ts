/// <reference types="vitest/config" />
import { fileURLToPath, URL } from 'node:url'

import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  build: {
    target: 'es2022',
    // Release policy (audit A-03): không đóng gói source map — giảm dung
    // lượng dist và không lộ implementation. Dev server vẫn map trực tiếp từ source.
    sourcemap: false,
    // chunk `three` ~746kB là lazy feature chunk — đừng cảnh báo nhiễu.
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      output: {
        // §153: không tạo giant bundle — tách shell khỏi các feature chunk lazy
        manualChunks: {
          'ui-shell': ['vue', 'vue-router', 'pinia'],
        },
      },
    },
  },
  test: {
    environment: 'jsdom',
    include: ['tests/**/*.test.ts'],
  },
})
