import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './app/App.vue'
import { router } from './app/router'

import './styles/tokens.css'

const app = createApp(App)
app.use(createPinia())
app.use(router)

// §48: splash có thời gian tối thiểu, không treo vô hạn; mount xong mới fade to dashboard
const startedAt = Date.now()
void router.isReady().then(() => {
  const elapsed = Date.now() - startedAt
  const wait = Math.max(0, 600 - elapsed)
  window.setTimeout(() => {
    app.mount('#app')
    // Pre-mount splash (index.html) fade-out rồi remove khỏi DOM
    const pre = document.getElementById('pre-splash')
    if (pre) {
      pre.classList.remove('on')
      window.setTimeout(() => pre.remove(), 300)
    }
  }, wait)
})
