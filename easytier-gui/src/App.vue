<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import pkg from '~/../package.json'

onBeforeMount(async () => {
  // 成员列表独立窗口保留自身标题「成员列表」，不覆盖。
  if (getCurrentWindow().label !== 'member') {
    await getCurrentWindow().setTitle(`ANF 平台架构 v${pkg.version}`)
  }
})
</script>

<template>
  <Toast position="bottom-right" />
  <RouterView />
</template>
