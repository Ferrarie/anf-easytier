<script setup lang="ts">
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { type } from '@tauri-apps/plugin-os'
import { useToast } from 'primevue'
import { computed, onMounted, ref } from 'vue'
import { useAnfFirstScreen } from '~/composables/anf_first_screen'
import { anfStatusMeta } from '~/composables/anf_status'
import { canOpenMemberWindow, toggleMemberWindow } from '~/composables/room_window'

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
// Windows 且当前进程未以管理员身份运行时，首页展示提示（仅非管理员时显示）。
const notAdmin = ref(false)
const { t } = useI18n()
const toast = useToast()

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

const statusMeta = computed(() => anfStatusMeta(status.value))

const heroSub = computed(() => {
  switch (status.value) {
    case 'pending':
      return '设备待审批，请联系管理员放行'
    case 'connecting':
      return '正在连接配置中心…'
    case 'failed':
      return '连接未建立，请检查地址或网络后重试'
    case 'connected':
      return networkName.value ? `已加入网络：${networkName.value}` : '已加入中心网络'
    default:
      return '填写服务器地址后点击启动，即可一键入网'
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

async function copyMachineId() {
  if (!machineId.value)
    return
  try {
    await writeText(machineId.value)
    toast.add({ severity: 'success', summary: '已复制', detail: '机器码已复制到剪贴板', life: 1500 })
  }
  catch (e) {
    toast.add({ severity: 'warn', summary: '复制失败', detail: e instanceof Error ? e.message : String(e), life: 2500 })
  }
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
  }
}
</script>

<template>
  <div class="anf-card overflow-hidden">
    <!-- 状态 Hero -->
    <div class="p-5 pb-0">
      <div class="flex items-center gap-3 rounded-2xl p-4" :class="`anf-hero-${statusMeta.tone}`">
        <div
          class="anf-gradient flex h-11 w-11 shrink-0 items-center justify-center rounded-xl text-lg text-white"
          :class="{ 'animate-pulse': statusMeta.pulse }"
        >
          <i :class="statusMeta.icon" />
        </div>
        <div class="min-w-0 flex-1">
          <div class="text-base font-semibold">
            {{ statusMeta.label }}
          </div>
          <div class="anf-muted truncate text-xs">
            {{ heroSub }}
          </div>
        </div>
        <Tag v-if="notAdmin" severity="warn" :value="t('admin.hint')" class="shrink-0" />
      </div>
      <Message v-if="errorMsg" severity="warn" :closable="true" class="mt-3 m-0">
        {{ errorMsg }}
      </Message>
    </div>

    <!-- 表单 -->
    <div class="flex flex-col gap-4 p-5">
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium">连接配置（自动保存）</label>
        <div class="flex items-center gap-2">
          <Select
            :model-value="activeIndex" :options="profileOptions" option-label="label" option-value="value"
            class="flex-1" @update:model-value="onSelectProfile"
          />
          <Button size="small" icon="pi pi-plus" severity="secondary" label="新建" class="shrink-0" @click="addProfile" />
          <Button
            size="small" icon="pi pi-trash" severity="danger" text label="删除" class="shrink-0"
            :disabled="profiles.length <= 1" @click="onRemoveCurrent"
          />
        </div>
        <small class="anf-muted">可保存多套中心地址，切换会先保存当前项；机器码不变</small>
      </div>

      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium">服务器地址</label>
        <InputText v-model="serverAddress" placeholder="例如 1.2.3.4:22020" />
        <small class="anf-muted">填一个可访问的公网/局域网 IP + 端口（支持 tcp/udp/ws）</small>
      </div>

      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium">设备昵称（自定义，可随时改）</label>
        <InputText v-model="nickname" placeholder="例如：我办公室的电脑" />
        <small class="anf-muted">会展示给同一网络内的其它成员</small>
      </div>

      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium">机器码</label>
        <div class="flex items-center gap-2 rounded-lg border border-[color:var(--anf-border)] bg-[color:var(--p-surface-100)] px-3 py-2">
          <code class="min-w-0 flex-1 truncate text-xs opacity-80">{{ machineId }}</code>
          <Button
            icon="pi pi-copy" text size="small" aria-label="复制机器码"
            class="shrink-0" @click="copyMachineId"
          />
        </div>
        <small class="anf-muted">设备唯一标识，不可修改；管理员以此审核放行</small>
      </div>

      <Button
        :label="runLabel" size="large"
        class="anf-gradient h-12 w-full border-0 text-base text-white shadow-lg shadow-indigo-500/25 hover:opacity-95"
        @click="onToggleRun"
      />

      <div class="border-t pt-3">
        <div class="flex flex-wrap items-center gap-x-4 gap-y-1">
          <Button text size="small" icon="pi pi-cog" :label="advancedOpen ? '收起高级' : '高级'" class="p-0 font-medium" @click="advancedOpen = !advancedOpen" />
          <Button
            text size="small" icon="pi pi-users" label="房间信息" class="p-0 font-medium"
            :disabled="!canOpenMemberWindow(status, lastInstanceId)"
            @click="toggleMemberWindow(status, lastInstanceId)"
          />
        </div>
        <div v-if="advancedOpen" class="mt-3 flex flex-col gap-3">
          <div class="flex flex-col gap-1">
            <label class="text-sm font-medium">网络名称</label>
            <InputText :model-value="networkName || '待下发'" readonly placeholder="由中心下发" />
            <small class="anf-muted">网络名由中心统一管理，客户端不可改（改了连不上中心）</small>
            <small v-if="lastInstanceId" class="anf-muted">最近实例：{{ lastInstanceId }}</small>
          </div>
          <div class="anf-muted flex flex-col gap-1 text-sm">
            <div>TUN 网卡名：anf_et（固定）</div>
            <div>配置源：由服务器地址自动生成</div>
            <div>网络密钥：中心下发，客户端不保存</div>
          </div>
          <div class="flex items-center gap-3">
            <Button label="立即保存" size="small" icon="pi pi-save" class="w-fit" @click="persist()" />
            <small class="anf-muted">更改会自动保存</small>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="postcss">
.anf-hero-neutral {
  background: #eceef4;
}
.anf-hero-accent {
  background: #eef2ff;
}
.anf-hero-warn {
  background: #fff7e6;
}
.anf-hero-success {
  background: #ecfdf5;
}
.anf-hero-danger {
  background: #fef2f2;
}

.p-dark .anf-hero-neutral {
  background: #22263a;
}
.p-dark .anf-hero-accent {
  background: #1c2140;
}
.p-dark .anf-hero-warn {
  background: #3a2f14;
}
.p-dark .anf-hero-success {
  background: #123524;
}
.p-dark .anf-hero-danger {
  background: #3b1a1e;
}
</style>
