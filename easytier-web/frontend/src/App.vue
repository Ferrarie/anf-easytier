<script setup lang="ts">
import { I18nUtils } from 'easytier-frontend-lib'
import { onMounted } from 'vue';
import { Toast, DynamicDialog } from 'primevue';

onMounted(async () => {
  // ANF 后台默认中文；避免被历史 localStorage(lang=en) 覆盖。
  localStorage.setItem('lang', 'cn')
  await I18nUtils.loadLanguageAsync('cn')
});

</script>

<!-- https://flowbite.com/docs/components/sidebar/#sidebar-with-navbar -->

<template>
  <Toast position="bottom-right" />
  <DynamicDialog />

  <RouterView />
</template>

<style scoped>
button {
  text-align: left;
  justify-content: left;
}
</style>
