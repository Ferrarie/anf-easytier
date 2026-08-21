<script setup lang="ts">
import { onMounted, computed, ref } from 'vue'
import { useAnfFirstScreen } from '~/composables/anf_first_screen'

const {
  inviteCode,
  serverAddress,
  networkName,
  status,
  machineId,
  errorMsg,
  init,
  persist,
  start,
  stop,
} = useAnfFirstScreen()

const advancedOpen = ref(false)

onMounted(async () => {
  await init()
})

const running = computed(() => {
  return ['connected', 'pending', 'connecting'].includes(status.value)
})

const runLabel = computed(() => (running.value ? '停止' : '启动'))
const runIcon = computed(() => (running.value ? 'pi-power-off' : 'pi-play'))

const statusText = computed(() => {
  switch (status.value) {
    case 'connecting':
      return '连接中…'
    case 'pending':
      return '设备待审批，请联系管理员放行'
    case 'connected':
      return '已连接'
    case 'failed':
      return '连接失败'
    default:
      return '未连接'
  }
})

async function onToggleRun() {
  errorMsg.value = undefined
  if (running.value) {
    await stop()
  }
  else {
    await persist()
    await start()
    if (errorMsg.value) {
      return
    }
  }
}
</script>

<template>
  <div class="anf-card mx-auto my-6 flex w-full max-w-xl flex-col gap-4 rounded-xl border p-6 shadow-sm">
    <div class="flex items-center gap-2">
      <i class="pi pi-globe text-primary" />
      <span class="text-lg font-semibold">ANF 快速连接</span>
      <span v-if="machineId" class="ml-auto text-xs text-secondary">设备 {{ machineId.slice(0, 8) }}</span>
    </div>

    <Message v-if="errorMsg" severity="warn" :closable="true">{{ errorMsg }}</Message>

    <div class="flex flex-col gap-1">
      <label class="text-sm font-medium">邀请码</label>
      <InputText v-model="inviteCode" placeholder="粘贴你的邀请码" />
    </div>

    <div class="flex flex-col gap-1">
      <label class="text-sm font-medium">服务器地址</label>
      <InputText v-model="serverAddress" placeholder="例如 10.0.0.6:22020" />
      <small class="text-secondary">填一个可访问的公网/局域网 IP + 端口（支持 tcp/udp/ws）</small>
    </div>

    <Button :label="runLabel" :icon="runIcon" size="large" class="w-full"
      :loading="status === 'connecting'" @click="onToggleRun" />

    <div class="text-center text-sm text-secondary">
      {{ statusText }}
    </div>

    <div class="border-t pt-3">
      <Button text size="small" icon="pi pi-cog" label="高级" class="p-0"
        @click="advancedOpen = !advancedOpen" />
      <div v-if="advancedOpen" class="mt-3 flex flex-col gap-3">
        <div class="flex flex-col gap-1">
          <label class="text-sm font-medium">网络名称</label>
          <InputText v-model="networkName" placeholder="网络名（默认由中心下发）" />
        </div>
        <div class="flex flex-col gap-1 text-sm text-secondary">
          <div>TUN 网卡名：anf_et（固定）</div>
          <div>配置源：由服务器地址自动生成</div>
        </div>
        <Button label="保存到本地" size="small" icon="pi pi-save" class="w-fit" @click="persist()" />
      </div>
    </div>
  </div>
</template>
