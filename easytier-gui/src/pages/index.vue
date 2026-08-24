<script setup lang="ts">
import type { MenuItem } from 'primevue/menuitem'

import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { type } from '@tauri-apps/plugin-os'
import { exit } from '@tauri-apps/plugin-process'
import { open } from '@tauri-apps/plugin-shell'
import { I18nUtils, Utils } from 'easytier-frontend-lib'
import { useToast } from 'primevue'
import pkg from '~/../package.json'
import AnfFirstScreen from '~/components/AnfFirstScreen.vue'

import { initMobileVpnService, syncMobileVpnService } from '~/composables/mobile_vpn'
import { loadMode, type Mode, saveMode } from '~/composables/mode'
import { useTray } from '~/composables/tray'

const { t, locale } = useI18n()
const toast = useToast()
const aboutVisible = ref(false)
const currentMode = ref<Mode>({ mode: 'normal' })

async function initWithMode(mode: Mode) {
  const running_inst_ids = (await listNetworkInstanceIds().catch(() => undefined))?.running_inst_ids ?? []

  const url: string | undefined = mode.rpc_portal
  const retrys = 1
  for (let i = 0; i < retrys; i++) {
    try {
      await connectRpcClient(mode.mode === 'normal', url)
      break
    }
    catch (e) {
      if (i === retrys - 1) {
        const errMsg = e instanceof Error ? e.message : String(e)
        toast.add({
          severity: 'error',
          summary: t('error'),
          detail: t('mode.rpc_connection_failed', { error: errMsg }),
          life: 1000,
        })
        throw e
      }
      console.error('Error connecting rpc client, retrying...', e)
      await new Promise(resolve => setTimeout(resolve, 1000))
    }
  }
  await sendConfigs(running_inst_ids.map(Utils.UuidToStr))
  if (mode.mode === 'normal') {
    mode.config_server_url = mode.config_server_url || undefined
    initWebClient(mode.config_server_url)
  }
  currentMode.value = mode
  saveMode(mode)
}

onMounted(async () => {
  const cleanupFns: Array<() => void> = []

  if (type() === 'android') {
    try {
      await initMobileVpnService()
    }
    catch (e: any) {
      console.error('easytier init vpn service failed', e)
    }
  }

  cleanupFns.push(await listenGlobalEvents())
  currentMode.value = loadMode()
  await initWithMode(currentMode.value)

  if (type() === 'android') {
    try {
      await syncMobileVpnService()
    }
    catch (e: any) {
      console.error('easytier sync vpn service failed', e)
    }
  }

  onUnmounted(() => {
    cleanupFns.forEach(unlisten => unlisten())
  })
})

useTray(true)

onMounted(async () => {
  window.setTimeout(async () => {
    await setTrayMenu([
      await MenuItemShow(t('tray.show')),
      await MenuItemExit(t('tray.exit')),
    ])
  }, 1000)
})

let current_log_level = 'off'

const log_menu = ref()
// 从后端获取正确的日志路径
async function getLogDirPath(): Promise<string> {
  return await invoke<string>('get_log_dir_path')
}

const log_menu_items_popup: Ref<MenuItem[]> = ref([
  ...['off', 'warn', 'info', 'debug', 'trace'].map(level => ({
    label: () => t(`logging_level_${level}`) + (current_log_level === level ? ' ✓' : ''),
    command: async () => {
      current_log_level = level
      await setLoggingLevel(level)
    },
  })),
  {
    separator: true,
  },
  {
    label: () => t('logging_open_dir'),
    icon: 'pi pi-folder-open',
    command: async () => {
      // console.log('open log dir', await getLogDirPath())
      await open(await getLogDirPath())
    },
    visible: () => type() !== 'android',
  },
  {
    label: () => t('logging_copy_dir'),
    icon: 'pi pi-tablet',
    command: async () => {
      await writeText(await getLogDirPath())
    },
  },
])

function toggle_log_menu(event: any) {
  log_menu.value.toggle(event)
}

function getLabel(item: MenuItem) {
  return typeof item.label === 'function' ? item.label() : item.label
}

const setting_menu_items: Ref<MenuItem[]> = ref([
  {
    label: () => t('exchange_language'),
    icon: 'pi pi-language',
    command: async () => {
      await I18nUtils.loadLanguageAsync((locale.value === 'en' ? 'cn' : 'en'))
      await setTrayMenu([
        await MenuItemShow(t('tray.show')),
        await MenuItemExit(t('tray.exit')),
      ])
    },
  },
  {
    key: 'logging_menu',
    label: () => t('logging'),
    icon: 'pi pi-file',
    items: [], // Keep this to show it's a parent menu
  },
  {
    label: () => t('about.title'),
    icon: 'pi pi-at',
    command: async () => {
      aboutVisible.value = true
    },
  },
  {
    label: () => t('exit'),
    icon: 'pi pi-power-off',
    command: async () => {
      await exit(1)
    },
  },
])

async function connectRpcClient(isNormalMode: boolean, url?: string) {
  await initRpcConnection(isNormalMode, url)
  console.warn('easytier rpc connection established, isNormalMode: ', isNormalMode)
}
</script>

<template>
  <div id="root" class="flex flex-col">
    <Dialog v-model:visible="aboutVisible" modal :header="t('about.title')" :style="{ width: '70%' }">
      <About />
    </Dialog>

    <Menu ref="log_menu" :model="log_menu_items_popup" :popup="true" />

    <Menubar :model="setting_menu_items" breakpoint="795px" class="shrink-0 border-x-0 border-t-0">
      <template #start>
        <div class="mr-6 flex items-center gap-2.5 py-1">
          <svg width="28" height="28" viewBox="0 0 28 28" fill="none" aria-hidden="true">
            <defs>
              <linearGradient id="anf-logo-g" x1="0" y1="0" x2="28" y2="28">
                <stop stop-color="#6366f1" />
                <stop offset="1" stop-color="#8b5cf6" />
              </linearGradient>
            </defs>
            <rect width="28" height="28" rx="8" fill="url(#anf-logo-g)" />
            <circle cx="9" cy="10" r="2.6" fill="#fff" />
            <circle cx="19" cy="8" r="2.2" fill="#fff" opacity=".85" />
            <circle cx="18" cy="19" r="2.8" fill="#fff" />
            <path d="M11.2 11.2 17 8.8M10.5 12.3l5.9 5.2M15.7 8.9 17.4 16.3" stroke="#fff" stroke-width="1.4" stroke-linecap="round" />
          </svg>
          <div class="flex flex-col leading-tight">
            <span class="text-sm font-semibold">ANF 平台架构</span>
            <span class="anf-muted text-xs">v{{ pkg.version }}</span>
          </div>
        </div>
      </template>
      <template #item="{ item, props }">
        <a v-if="item.key === 'logging_menu'" v-bind="props.action" @click="toggle_log_menu">
          <span :class="item.icon" />
          <span class="p-menubar-item-label">{{ getLabel(item) }}</span>
          <span class="pi pi-angle-down p-menubar-item-icon text-[9px]" />
        </a>
        <a v-else v-bind="props.action">
          <span :class="item.icon" />
          <span class="p-menubar-item-label">{{ getLabel(item) }}</span>
        </a>
      </template>
    </Menubar>

    <main class="min-h-0 flex-1 overflow-y-auto">
      <AnfFirstScreen class="mx-auto w-full max-w-xl px-4 py-5" />
    </main>
  </div>
</template>

<style scoped lang="postcss">
#root {
  height: 100vh;
  width: 100vw;
}

.p-dropdown :deep(.p-dropdown-panel .p-dropdown-items .p-dropdown-item) {
  padding: 0 0.5rem;
}
</style>

<style>
body {
  height: 100vh;
  width: 100vw;
  padding: 0;
  margin: 0;
  overflow: hidden;
}

.p-menubar .p-menuitem {
  margin: 0;
}

.p-select-overlay {
  max-width: calc(100% - 2rem);
}

/*

.p-tabview-panel {
  height: 100%;
} */
</style>
