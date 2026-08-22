<script setup lang="ts">
import { onMounted, computed, ref } from 'vue'
import { type } from '@tauri-apps/plugin-os'
import { useAnfFirstScreen } from '~/composables/anf_first_screen'
import AnfMemberList from '~/components/AnfMemberList.vue'

const {
  profiles,
  activeIndex,
  serverAddress,
  nickname,
  status,
  machineId,
  networkName,
  lastInstanceId,
  errorMsg,
  init,
  persist,
  start,
  stop,
  addProfile,
  removeProfile,
  switchProfile,
} = useAnfFirstScreen()

const advancedOpen = ref(false)
const membersOpen = ref(false)
// Windows 且当前进程未以管理员身份运行时，首页展示提示（仅非管理员时显示）。
const notAdmin = ref(false)
const { t } = useI18n()

onMounted(async () => {
  await init()
  if (type() === 'windows') {
    try {
      notAdmin.value = !(await isAdmin())
    }
    catch (e) {
      console.warn('is_admin check failed', e)
      notAdmin.value = false
    }
  }
})

const running = computed(() => {
  return ['connected', 'pending', 'connecting'].includes(status.value)
})

const runLabel = computed(() => {
  switch (status.value) {
    case 'connecting':
      return '连接中…'
    case 'pending':
      return '审核中…'
    case 'connected':
      return '停止'
    case 'failed':
      return '重试'
    default:
      return '启动'
  }
})

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

const profileOptions = computed(() =>
  profiles.value.map((p, i) => ({ label: p.name || `配置${i + 1}`, value: i })),
)

/** 切换配置档案；使用 :model-value + update:model-value，避免 v-model 提前改 activeIndex。 */
function onSelectProfile(value: number) {
  void switchProfile(value)
}

function onRemoveCurrent() {
  removeProfile(activeIndex.value)
}

async function onToggleRun() {
  errorMsg.value = undefined
  if (running.value) {
    await stop()
  }
  else {
    // 保存失败不阻塞连接（例如 exe 目录只读时仍尝试启动）。
    try {
      await persist()
    }
    catch (e) {
      console.warn('persist config failed, continuing to start', e)
    }
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
    </div>

    <Message v-if="notAdmin" severity="warn" :closable="false" class="m-0">
      {{ t('admin.hint') }}
    </Message>

    <Message v-if="errorMsg" severity="warn" :closable="true">{{ errorMsg }}</Message>

    <div class="flex flex-col gap-1">
      <label class="text-sm font-medium">连接配置（自动保存）</label>
      <div class="flex items-center gap-2">
        <Select :model-value="activeIndex" :options="profileOptions" option-label="label" option-value="value"
          class="flex-1" @update:model-value="onSelectProfile" />
        <Button size="small" icon="pi pi-plus" severity="secondary" label="新建" @click="addProfile" />
        <Button size="small" icon="pi pi-trash" severity="danger" text label="删除"
          :disabled="profiles.length <= 1" @click="onRemoveCurrent" />
      </div>
      <small class="text-secondary">可保存多套中心地址，切换会先保存当前项；机器码不变</small>
    </div>

    <div class="flex flex-col gap-1">
      <label class="text-sm font-medium">服务器地址</label>
      <InputText v-model="serverAddress" placeholder="例如 10.0.0.6:22020" />
      <small class="text-secondary">填一个可访问的公网/局域网 IP + 端口（支持 tcp/udp/ws）</small>
    </div>

    <div class="flex flex-col gap-1">
      <label class="text-sm font-medium">设备昵称（自定义，可随时改）</label>
      <InputText v-model="nickname" placeholder="例如：我办公室的电脑" />
      <small class="text-secondary">会展示给同一网络内的其它成员</small>
    </div>

    <div class="flex flex-col gap-1">
      <label class="text-sm font-medium">机器码</label>
      <InputText :model-value="machineId" readonly class="opacity-80" />
      <small class="text-secondary">设备唯一标识，不可修改；管理员以此审核放行</small>
    </div>

    <Button :label="runLabel" size="large" class="w-full h-12 text-base" @click="onToggleRun" />

    <div class="text-center text-sm text-secondary">
      {{ statusText }}
    </div>

    <div class="border-t pt-3">
      <Button text size="small" icon="pi pi-cog" label="高级" class="p-0"
        @click="advancedOpen = !advancedOpen" />
      <div v-if="advancedOpen" class="mt-3 flex flex-col gap-3">
        <div class="flex flex-col gap-1">
          <label class="text-sm font-medium">网络名称</label>
          <InputText :model-value="networkName || '待下发'" readonly placeholder="由中心下发" />
          <small class="text-secondary">网络名由中心统一管理，客户端不可改（改了连不上中心）</small>
          <small v-if="lastInstanceId" class="text-secondary">最近实例：{{ lastInstanceId }}</small>
        </div>
        <div class="flex flex-col gap-1 text-sm text-secondary">
          <div>TUN 网卡名：anf_et（固定）</div>
          <div>配置源：由服务器地址自动生成</div>
          <div>网络密钥：中心下发，客户端不保存</div>
        </div>
        <div class="flex items-center gap-3">
          <Button label="立即保存" size="small" icon="pi pi-save" class="w-fit" @click="persist()" />
          <small class="text-secondary">更改会自动保存</small>
        </div>
      </div>
    </div>

    <div class="border-t pt-3">
      <Button text size="small" icon="pi pi-users" label="成员列表" class="p-0"
        @click="membersOpen = !membersOpen" />
      <div v-if="membersOpen" class="mt-3">
        <AnfMemberList :instance-id="lastInstanceId" />
      </div>
    </div>
  </div>
</template>
