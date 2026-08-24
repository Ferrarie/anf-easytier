import Aura from '@primeuix/themes/aura'
import { definePreset } from '@primeuix/themes'
import EasyTierFrontendLib, { I18nUtils } from 'easytier-frontend-lib'

import { ConfirmationService, DialogService, ToastService } from 'primevue'
import PrimeVue from 'primevue/config'

// vue-router/auto 与 auto-routes 是不同虚拟模块，TS 依赖分开导入；no-duplicates 会按物理文件判重，故就地豁免。
/* eslint-disable import/no-duplicates */
import { createRouter, createWebHashHistory } from 'vue-router/auto'
import { routes } from 'vue-router/auto-routes'
/* eslint-enable import/no-duplicates */

import App from '~/App.vue'
import 'easytier-frontend-lib/style.css'
import '~/styles.css'

// ANF 品牌主色：indigo 色阶（与 #6366f1→#8b5cf6 渐变同族，深浅模式自动适配）
const AnfPreset = definePreset(Aura, {
  semantic: {
    primary: {
      50: '#eef2ff',
      100: '#e0e7ff',
      200: '#c7d2fe',
      300: '#a5b4fc',
      400: '#818cf8',
      500: '#6366f1',
      600: '#4f46e5',
      700: '#4338ca',
      800: '#3730a3',
      900: '#312e81',
      950: '#1e1b4b',
    },
  },
})

if (import.meta.env.PROD) {
  document.addEventListener('keydown', (event) => {
    if (
      event.key === 'F5'
      || (event.ctrlKey && event.key === 'r')
      || (event.metaKey && event.key === 'r')
    ) {
      event.preventDefault()
    }
  })

  document.addEventListener('contextmenu', (event) => {
    event.preventDefault()
  })
}

async function main() {
  await I18nUtils.loadLanguageAsync(localStorage.getItem('lang') || 'en')

  const app = createApp(App)

  const router = createRouter({
    history: createWebHashHistory(),
    routes,
  })

  app.use(router)
  app.use(createPinia())
  app.use(EasyTierFrontendLib)
  // app.use(i18n, { useScope: 'global' })
  app.use(PrimeVue, {
    theme: {
      preset: AnfPreset,
      options: {
        prefix: 'p',
        darkModeSelector: 'system',
        cssLayer: {
          name: 'primevue',
          order: 'tailwind-base, primevue, tailwind-utilities',
        },
      },
    },
  })
  app.use(ToastService)
  app.use(DialogService)
  app.use(ConfirmationService)
  app.mount('#app')
}

main()
