import { ref } from 'vue'
import {
  anfLoadConfig,
  anfSaveConfig,
  anfGetMachineId,
  anfNormalizeAddress,
} from './backend'

export type AnfStatus = 'idle' | 'connecting' | 'pending' | 'connected' | 'failed'
export type InviteStatus = 'pending' | 'approved' | 'used' | 'revoked'

export interface AnfConfig {
  schema_version: number
  machine_id?: string
  server_address?: string
  network_name?: string
  invite_code?: string
  invite_status: InviteStatus
  last_instance_id?: string
}

export function useAnfFirstScreen() {
  const inviteCode = ref('')
  const serverAddress = ref('')
  const networkName = ref('')
  const status = ref<AnfStatus>('idle')
  const machineId = ref<string | undefined>(undefined)
  const errorMsg = ref<string | undefined>(undefined)
  const configPath = ref<string | undefined>(undefined)

  /** 启动/刷新：载入本地配置，保证存在稳定机器 ID。 */
  async function init() {
    const raw = await anfLoadConfig()
    const cfg = (JSON.parse(raw || '{}') || {}) as Partial<AnfConfig>
    inviteCode.value = cfg.invite_code ?? ''
    serverAddress.value = cfg.server_address ?? ''
    networkName.value = cfg.network_name ?? ''
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
      network_name: networkName.value.trim() || undefined,
      invite_code: inviteCode.value.trim() || undefined,
      invite_status: 'pending',
      last_instance_id: undefined,
    }
    configPath.value = await anfSaveConfig(cfg)
    return configPath.value
  }

  /** 启动：Phase C 将在此实现 注册→审批→连网。当前为占位状态机。 */
  async function start() {
    status.value = 'connecting'
    errorMsg.value = undefined
    const addr = await normalizeAddress()
    if (!addr) {
      status.value = 'idle'
      return
    }
    status.value = 'pending' // 占位：待 Phase C 接入真实链路
  }

  /** 停止：Phase C 在此实现停止实例。 */
  async function stop() {
    status.value = 'idle'
  }

  return {
    inviteCode,
    serverAddress,
    networkName,
    status,
    machineId,
    errorMsg,
    configPath,
    init,
    normalizeAddress,
    persist,
    start,
    stop,
  }
}
