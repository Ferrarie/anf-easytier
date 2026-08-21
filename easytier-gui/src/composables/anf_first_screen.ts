import { ref } from 'vue'
import {
  anfLoadConfig,
  anfSaveConfig,
  anfGetMachineId,
  anfNormalizeAddress,
  initWebClient,
  isWebClientConnected,
  listNetworkInstanceIds,
} from './backend'

export type AnfStatus = 'idle' | 'connecting' | 'pending' | 'connected' | 'failed'

export interface AnfConfig {
  schema_version: number
  machine_id?: string
  server_address?: string
  nickname?: string
  last_instance_id?: string
}

export function useAnfFirstScreen() {
  const serverAddress = ref('')
  const nickname = ref('')
  const status = ref<AnfStatus>('idle')
  const machineId = ref<string | undefined>(undefined)
  const errorMsg = ref<string | undefined>(undefined)
  const configPath = ref<string | undefined>(undefined)
  let pollTimer: ReturnType<typeof setInterval> | null = null

  /** 启动/刷新：载入本地配置，保证存在稳定机器 ID。 */
  async function init() {
    const raw = await anfLoadConfig()
    const cfg = (JSON.parse(raw || '{}') || {}) as Partial<AnfConfig>
    serverAddress.value = cfg.server_address ?? ''
    nickname.value = cfg.nickname ?? ''
    machineId.value = cfg.machine_id ?? (await anfGetMachineId())
    errorMsg.value = undefined
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
    const cfg: AnfConfig = {
      schema_version: 1,
      machine_id: machineId.value,
      server_address: serverAddress.value.trim() || undefined,
      nickname: nickname.value.trim() || undefined,
      last_instance_id: undefined,
    }
    configPath.value = await anfSaveConfig(cfg)
    return configPath.value
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
    } catch (e) {
      status.value = 'failed'
      errorMsg.value = e instanceof Error ? e.message : String(e)
      return
    }
    status.value = 'pending' // 已连上 config-server，等待服务端放行下发配置
    // 轮询：连上且已有实例视为已连接，否则保持 pending（等待审批）。
    await pollStatusOnce()
    if (pollTimer) clearInterval(pollTimer)
    pollTimer = setInterval(pollStatusOnce, 2000)
  }

  async function pollStatusOnce() {
    try {
      const connected = await isWebClientConnected()
      if (!connected) {
        // 还没连上 config-server，可能仍在重试。
        if (status.value !== 'failed') status.value = 'connecting'
        return
      }
      // 已连上：若已有运行实例（拿到托管配置建了 TUN）才算真正联网。
      const { running_inst_ids } = await listNetworkInstanceIds()
      if (running_inst_ids && running_inst_ids.length > 0) {
        status.value = 'connected'
        cleanup()
      } else {
        status.value = 'pending'
      }
    } catch (e) {
      // 轮询失败不改变已连上状态，仅在 idle/connecting 时标记失败。
      if (status.value !== 'connected') {
        status.value = 'failed'
        errorMsg.value = e instanceof Error ? e.message : String(e)
        cleanup()
      }
    }
  }

  /** 停止：断开 config-server 连接。 */
  async function stop() {
    cleanup()
    try {
      await initWebClient(undefined, undefined, undefined)
    } catch {
      // 忽略断开错误
    }
    status.value = 'idle'
  }

  return {
    serverAddress,
    nickname,
    status,
    machineId,
    errorMsg,
    configPath,
    init,
    normalizeAddress,
    persist,
    start,
    stop,
    cleanup,
  }
}
