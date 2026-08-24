<script setup lang="ts">
import { getEasytierVersion } from '~/composables/backend'
import pkg from '~/../package.json'
import { formatVersionDisplay } from '~/composables/version'

const { t } = useI18n()

const coreVersion = ref('')

onMounted(async () => {
  coreVersion.value = await getEasytierVersion()
})

const display = computed(() => formatVersionDisplay(pkg.version, coreVersion.value))
</script>

<template>
  <Card>
    <template #title>
      ANF 平台架构
    </template>
    <template #content>
      <p class="mb-1">
        {{ t('about.description') }}
      </p>
      <div class="mt-3 space-y-1 text-sm">
        <p>{{ t('about.gui_version') }}: {{ display.gui }}</p>
        <p>{{ t('about.core_version') }}: {{ display.core }}</p>
      </div>
    </template>
  </Card>
</template>

<style scoped lang="postcss">
</style>
