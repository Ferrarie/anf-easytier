<script setup lang="ts">
import { onMounted, ref } from 'vue'
import AnfMemberList from '~/components/AnfMemberList.vue'
import { anfLoadConfig, collectNetworkInfo } from '~/composables/backend'
import { networkCidr } from '~/composables/ip_cidr'

// 成员窗口是独立 WebviewWindow，无法直接跨窗口读主屏 ref；从本地配置取当前档案的实例 id。
const instanceId = ref<string>('')
const cidr = ref('')

onMounted(async () => {
  try {
    const cfg = JSON.parse(await anfLoadConfig()) as {
      active_profile_index?: number
      profiles?: Array<{ last_instance_id?: string }>
    }
    const index = cfg.active_profile_index ?? 0
    instanceId.value = cfg.profiles?.[index]?.last_instance_id ?? ''
    if (instanceId.value) {
      const info = await collectNetworkInfo(instanceId.value)
      cidr.value = networkCidr(info?.info?.map?.[instanceId.value])
    }
  }
  catch (e) {
    console.warn('member window load config failed', e)
  }
})
</script>

<template>
  <div class="h-full w-full overflow-auto p-3">
    <div class="anf-card overflow-hidden">
      <div class="flex items-center justify-between border-b px-4 py-3">
        <div class="flex items-center gap-2">
          <i class="pi pi-users text-primary" />
          <span class="text-sm font-semibold">成员列表</span>
        </div>
        <Tag v-if="cidr" severity="info" :value="`网段 ${cidr}`" />
      </div>
      <div class="p-2">
        <AnfMemberList :instance-id="instanceId" />
      </div>
    </div>
  </div>
</template>
