import { getCurrentInstance, onUnmounted, ref, watch } from 'vue'
import {
  anfGetMachineId,
  anfLoadConfig,
  anfNormalizeAddress,
  anfSaveConfig,
  deleteNetworkInstance,
  getNetworkMetas,
  initWebClient,
  isWebClientConnected,
  listNetworkInstanceIds,
} from './backend'

// 兼容 proto 上报的 UUID（{part1..part4}）与直达字符串，本地实现避免引入 frontend-lib 重依赖。
function instanceIdToStr(id: unknown): string {
  if (typeof id === 'string' && id.trim()) {
    return id
  }
  if (id && typeof id === 'object') {
    const u = id as { part1?: number, part2?: number, part3?: number, part4?: number }
    if (
      typeof u.part1 === 'number' && typeof u.part2 === 'number'
      && typeof u.part3 === 'number' && typeof u.part4 === 'number'
    ) {
      const hex = (n: number) => BigInt(n).toString(16).padStart(8, '0')
      const p1 = hex(u.part1)
      const p2 = hex(u.part2)
      const p3 = hex(u.part3)
      const p4 = hex(u.part4)
      return `${p1.slice(0, 8)}-${p2.slice(0, 4)}-${p2.slice(4, 8)}-${p3.slice(0, 4)}-${p3.slice(4, 8)}${p4.slice(0, 12)}`
    }
  }
  return ''
}

export type AnfStatus = 'idle' | 'connecting' | 'pending' | 'connected' | 'failed'

/** 单个连接配置档案：一个中心服务器地址 + 该中心的昵称等信息。 */
export interface AnfProfile {
  name?: string
  server_address?: string
  nickname?: string
  network_name?: string
  last_instance_id?: string
}

/** 本地配置（多档案）。machine_id 为整机级字段。 */
export interface AnfConfig {
  schema_version: number
  machine_id?: string
  active_profile_index?: number
  profiles?: AnfProfile[]
}

// 模块级标记：仅首次挂载时自动重连，避免切换页面/重复挂载导致反复连。
let autoStarted = false

function clampIndex(value: number, max: number): number {
  if (!Number.isFinite(value)) {
    return 0
  }
  const n = Math.trunc(value)
  if (n < 0)
    return 0
  if (n > max)
    return max
  return n
}

export function useAnfFirstScreen() {
  // 初始保留一个默认档案，保证未 init 时 addProfile/persist 也成立（真实使用前会 init 覆盖）。
  const profiles = ref<AnfProfile[]>([{ name: '默认' }])
  const activeIndex = ref(0)
  const serverAddress = ref('')
  const nickname = ref('')
  const status = ref<AnfStatus>('idle')
  const machineId = ref<string | undefined>(undefined)
  const networkName = ref<string | undefined>(undefined)
  const lastInstanceId = ref<string | undefined>(undefined)
  const errorMsg = ref<string | undefined>(undefined)
  const configPath = ref<string | undefined>(undefined)
  let pollTimer: ReturnType<typeof setInterval> | null = null
  let saveTimer: ReturnType<typeof setTimeout> | null = null

  /** 改动后延迟落盘（自动保存，去掉显式“保存”的隐性认知负担）。 */
  function schedulePersist() {
    if (!machineId.value) {
      return
    }
    if (saveTimer) {
      clearTimeout(saveTimer)
    }
    saveTimer = setTimeout(() => {
      persist().catch(e => console.warn('anf autosave failed', e))
    }, 400)
  }

  // 地址 / 昵称 / 当前档案变化即自动保存；切换档案在 switchProfile 内已先捕获。
  watch([serverAddress, nickname, activeIndex], () => schedulePersist())

  // 仅在组件实例内注册（测试直接调用 composable 时无实例，避免生命周期告警）。
  if (getCurrentInstance()) {
    onUnmounted(() => {
      if (saveTimer) {
        clearTimeout(saveTimer)
        saveTimer = null
      }
    })
  }

  /** 当前档案对象；越界时返回默认档案。 */
  function currentProfile(): AnfProfile {
    return profiles.value[activeIndex.value] ?? { name: '默认' }
  }

  /** 用当前档案回填编辑器字段。 */
  function applyActiveProfile() {
    const p = currentProfile()
    serverAddress.value = p.server_address ?? ''
    nickname.value = p.nickname ?? ''
    networkName.value = p.network_name
    lastInstanceId.value = p.last_instance_id
  }

  /** 把编辑器字段写回当前档案。 */
  function captureActiveProfile() {
    const p = currentProfile()
    p.server_address = serverAddress.value.trim() || undefined
    p.nickname = nickname.value.trim() || undefined
    p.network_name = networkName.value
    p.last_instance_id = lastInstanceId.value
    if (!p.name) {
      p.name = `配置${activeIndex.value + 1}`
    }
  }

  /** 启动/刷新：载入本地配置，保证存在稳定机器 ID，并回填上次成功连接的配置。 */
  async function init() {
    const raw = await anfLoadConfig()
    const cfg = (JSON.parse(raw || '{}') || {}) as Partial<AnfConfig>
    profiles.value = cfg.profiles && cfg.profiles.length > 0
      ? cfg.profiles
      : [{ name: '默认' }]
    activeIndex.value = clampIndex(cfg.active_profile_index ?? 0, profiles.value.length - 1)
    // 机器码由硬件标识推导（同机不变），始终从后端取最新值，避免沿用旧随机 UUID。
    machineId.value = await anfGetMachineId()
    applyActiveProfile()
    errorMsg.value = undefined

    // 上次成功连接过（有 last_instance_id）：自动回填并尝试恢复连接。
    if (serverAddress.value.trim() && lastInstanceId.value && !autoStarted) {
      autoStarted = true
      await start()
    }

    return cfg
  }

  /** 把地址框内容归一化为配置源 URL；失败返回 undefined 并给出人话错误。 */
  async function normalizeAddress(): Promise<string | undefined> {
    if (!serverAddress.value.trim()) {
      errorMsg.value = '请填写服务器地址'
      return undefined
    }
    try {
      return await anfNormalizeAddress(serverAddress.value.trim())
    }
    catch (e) {
      errorMsg.value = e instanceof Error ? e.message : String(e)
      return undefined
    }
  }

  /** 保存当前首屏状态（非机密）到 exe 同目录 config.toml。 */
  async function persist() {
    captureActiveProfile()
    const cfg: AnfConfig = {
      schema_version: 2,
      machine_id: machineId.value,
      active_profile_index: activeIndex.value,
      profiles: profiles.value,
    }
    configPath.value = await anfSaveConfig(cfg)
    return configPath.value
  }

  /** 连接成功后补全网络名/最近实例 ID，并落盘。 */
  async function captureAndPersist(runningIds: string[]) {
    if (runningIds.length === 0) {
      return
    }
    // 用最近运行实例回填实例 ID 与网络名（由中心下发的真实网络名）。
    lastInstanceId.value = runningIds[0]
    try {
      const resp = await getNetworkMetas(runningIds)
      const meta = resp?.metas?.[runningIds[0]]
      if (meta?.network_name) {
        networkName.value = meta.network_name
      }
    }
    catch {
      // 网络名拉取失败不阻塞连接；保留已有/默认值。
    }
    await persist()
  }

  /** 新增一个空配置档案并切换过去。 */
  function addProfile() {
    // 先把当前档案已填内容回写，避免切走时丢失。
    captureActiveProfile()
    const p: AnfProfile = {
      name: `配置${profiles.value.length + 1}`,
      server_address: '',
      nickname: '',
    }
    profiles.value.push(p)
    activeIndex.value = profiles.value.length - 1
    applyActiveProfile()
    schedulePersist()
  }

  /** 删除指定档案（至少保留一个），并修正当前索引。 */
  function removeProfile(index: number) {
    if (profiles.value.length <= 1) {
      return
    }
    profiles.value.splice(index, 1)
    if (activeIndex.value >= profiles.value.length) {
      activeIndex.value = profiles.value.length - 1
    }
    else if (activeIndex.value > index) {
      activeIndex.value -= 1
    }
    applyActiveProfile()
    schedulePersist()
  }

  /** 切换配置档案；若当前已连接则先断开，避免连到旧中心。 */
  async function switchProfile(index: number) {
    if (index < 0 || index >= profiles.value.length || index === activeIndex.value) {
      return
    }
    if (['connected', 'pending', 'connecting'].includes(status.value)) {
      await stop()
    }
    // 切换前先保存当前档案，避免丢失未保存编辑。
    captureActiveProfile()
    activeIndex.value = index
    applyActiveProfile()
    schedulePersist()
  }

  /** 停止后台轮询（组件卸载时调用）。 */
  function cleanup() {
    if (pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  /** 启动：机器码接入 → 等待审批 → 连网。连 config-server，服务端管理登记/审批/下发管线。 */
  async function start() {
    status.value = 'connecting'
    errorMsg.value = undefined
    const addr = await normalizeAddress()
    if (!addr) {
      status.value = 'idle'
      return
    }
    if (!machineId.value) {
      machineId.value = await anfGetMachineId()
    }
    try {
      // machine_id 固定，hostname 用昵称（display_name，广播给其它成员）。
      await initWebClient(addr, machineId.value, nickname.value.trim() || undefined)
    }
    catch (e) {
      status.value = 'failed'
      errorMsg.value = e instanceof Error ? e.message : String(e)
      return
    }
    status.value = 'pending' // 已连上 config-server，等待服务端放行下发配置
    // 轮询：连上且已有实例视为已连接（终态），否则保持 pending（等待审批）。
    const terminal = await pollStatusOnce()
    if (terminal) {
      return
    }
    if (pollTimer)
      clearInterval(pollTimer)
    pollTimer = setInterval(async () => {
      const done = await pollStatusOnce()
      if (done) {
        cleanup()
      }
    }, 2000)
  }

  /** 轮询一次；返回是否进入终态（connected 或 failed），供 start 决定是否继续轮询。 */
  async function pollStatusOnce(): Promise<boolean> {
    try {
      const connected = await isWebClientConnected()
      if (!connected) {
        // 还没连上 config-server，可能仍在重试。
        if (status.value !== 'failed')
          status.value = 'connecting'
        return false
      }
      // 已连上：若已有运行实例（拿到托管配置建了 TUN）才算真正联网。
      const { running_inst_ids } = await listNetworkInstanceIds()
      const runningIds = (running_inst_ids ?? [])
        .map(instanceIdToStr)
        .filter(id => id.length > 0)
      if (runningIds.length > 0) {
        status.value = 'connected'
        await captureAndPersist(runningIds)
        return true
      }
      else {
        status.value = 'pending'
        return false
      }
    }
    catch (e) {
      // 轮询失败不改变已连上状态，仅在 idle/connecting 时标记失败。
      if (status.value !== 'connected') {
        status.value = 'failed'
        errorMsg.value = e instanceof Error ? e.message : String(e)
        return true
      }
      return false
    }
  }

  /** 停止：断开 config-server 连接，并停止/删除由服务器下发的运行实例（虚拟网卡随之消失）。 */
  async function stop() {
    cleanup()
    try {
      await initWebClient(undefined, undefined, undefined)
    }
    catch {
      // 忽略断开错误
    }
    try {
      const { running_inst_ids } = await listNetworkInstanceIds()
      for (const raw of running_inst_ids ?? []) {
        const id = instanceIdToStr(raw)
        if (!id)
          continue
        try {
          await deleteNetworkInstance(id)
        }
        catch (e) {
          console.warn('anf stop: remove network instance failed', id, e)
        }
      }
    }
    catch (e) {
      console.warn('anf stop: list network instances failed', e)
    }
    status.value = 'idle'
  }

  return {
    profiles,
    activeIndex,
    serverAddress,
    nickname,
    status,
    machineId,
    networkName,
    lastInstanceId,
    errorMsg,
    configPath,
    init,
    normalizeAddress,
    persist,
    start,
    stop,
    cleanup,
    addProfile,
    removeProfile,
    switchProfile,
  }
}
