import { describe, expect, it, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  anfLoadConfig: vi.fn(),
  anfSaveConfig: vi.fn(),
  anfGetMachineId: vi.fn(),
  anfNormalizeAddress: vi.fn(),
  initWebClient: vi.fn(),
  isWebClientConnected: vi.fn(),
  listNetworkInstanceIds: vi.fn(),
  getNetworkMetas: vi.fn(),
  deleteNetworkInstance: vi.fn(),
}))

vi.mock('./backend', () => mocks)

import { useAnfFirstScreen } from './anf_first_screen'

function makeCfg(overrides: Record<string, unknown> = {}) {
  return {
    schema_version: 2,
    machine_id: 'm-1',
    active_profile_index: 0,
    profiles: [{ name: '默认', server_address: '', nickname: '' }],
    ...overrides,
  }
}

describe('useAnfFirstScreen', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.anfGetMachineId.mockResolvedValue('m-1')
    mocks.anfSaveConfig.mockResolvedValue('C:/app/config.toml')
    mocks.anfNormalizeAddress.mockResolvedValue('tcp://10.0.0.1:22020')
    mocks.initWebClient.mockResolvedValue(undefined)
    mocks.isWebClientConnected.mockResolvedValue(false)
    mocks.listNetworkInstanceIds.mockResolvedValue({ running_inst_ids: [], disabled_inst_ids: [] })
    mocks.getNetworkMetas.mockResolvedValue({ metas: {} })
    mocks.deleteNetworkInstance.mockResolvedValue(undefined)
  })

  it('init 载入配置并补齐机器 ID', async () => {
    mocks.anfLoadConfig.mockResolvedValue(JSON.stringify(makeCfg({
      profiles: [{ name: '默认', server_address: '10.0.0.1:22020', nickname: '小白-办公' }],
    })))
    mocks.anfGetMachineId.mockResolvedValue('m-hw')
    const s = useAnfFirstScreen()
    await s.init()
    expect(s.serverAddress.value).toBe('10.0.0.1:22020')
    expect(s.machineId.value).toBe('m-hw')
    expect(s.nickname.value).toBe('小白-办公')
    expect(mocks.anfGetMachineId).toHaveBeenCalled()
  })

  it('init 上次成功连接时自动回填并重连', async () => {
    mocks.anfLoadConfig.mockResolvedValue(JSON.stringify(makeCfg({
      machine_id: 'm-existing',
      profiles: [{
        name: '默认',
        server_address: '10.0.0.1:22020',
        network_name: 'anf-m3',
        last_instance_id: 'i1',
      }],
    })))
    mocks.isWebClientConnected.mockResolvedValue(true)
    mocks.listNetworkInstanceIds.mockResolvedValue({ running_inst_ids: ['i1'], disabled_inst_ids: [] })
    mocks.getNetworkMetas.mockResolvedValue({ metas: { i1: { network_name: 'anf-m3' } } })
    const s = useAnfFirstScreen()
    await s.init()
    expect(s.serverAddress.value).toBe('10.0.0.1:22020')
    expect(s.networkName.value).toBe('anf-m3')
    expect(s.lastInstanceId.value).toBe('i1')
    expect(s.status.value).toBe('connected')
    s.cleanup()
  })

  it('init 无配置时生成机器 ID 且默认一个档案', async () => {
    mocks.anfLoadConfig.mockResolvedValue('{}')
    const s = useAnfFirstScreen()
    await s.init()
    expect(s.machineId.value).toBe('m-1')
    expect(s.profiles.value).toHaveLength(1)
    expect(s.activeIndex.value).toBe(0)
    expect(mocks.anfGetMachineId).toHaveBeenCalled()
  })

  it('normalizeAddress 调用后端并返回标准化地址', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = '10.0.0.1:22020'
    const addr = await s.normalizeAddress()
    expect(mocks.anfNormalizeAddress).toHaveBeenCalledWith('10.0.0.1:22020')
    expect(addr).toBe('tcp://10.0.0.1:22020')
  })

  it('normalizeAddress 空地址给出人话错误', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = ''
    const addr = await s.normalizeAddress()
    expect(addr).toBeUndefined()
    expect(s.errorMsg.value).toBe('请填写服务器地址')
  })

  it('persist 保存为多档案结构且不含密钥', async () => {
    mocks.anfSaveConfig.mockResolvedValue('C:/app/config.toml')
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    s.nickname.value = '办公室'
    const path = await s.persist()
    expect(path).toBe('C:/app/config.toml')
    const saved = mocks.anfSaveConfig.mock.calls[0][0] as Record<string, unknown>
    const profiles = saved.profiles as Array<Record<string, unknown>>
    expect(saved.active_profile_index).toBe(0)
    expect(profiles[0].server_address).toBe('1.2.3.4:22020')
    expect(profiles[0].nickname).toBe('办公室')
    // 配置结构不含网络密钥字段
    expect(JSON.stringify(saved)).not.toMatch(/secret|password|network_key/i)
  })

  it('start 发起 WebClient 连接并用机器ID+昵称', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    s.machineId.value = 'm-1'
    s.nickname.value = '办公室'
    mocks.isWebClientConnected.mockResolvedValue(true)
    mocks.listNetworkInstanceIds.mockResolvedValue({ running_inst_ids: ['i1'], disabled_inst_ids: [] })
    await s.start()
    expect(mocks.anfNormalizeAddress).toHaveBeenCalledWith('1.2.3.4:22020')
    expect(mocks.initWebClient).toHaveBeenCalledWith('tcp://10.0.0.1:22020', 'm-1', '办公室')
  })

  it('start 走 pending，等已连接且有实例后转 connected', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    s.machineId.value = 'm-1'
    mocks.isWebClientConnected.mockResolvedValue(true)
    mocks.listNetworkInstanceIds.mockResolvedValue({ running_inst_ids: ['i1'], disabled_inst_ids: [] })
    await s.start()
    expect(s.status.value).toBe('connected')
    s.cleanup()
  })

  it('start 无地址给出人话错误并回 idle', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = ''
    mocks.anfNormalizeAddress.mockResolvedValue(undefined)
    await s.start()
    expect(s.status.value).toBe('idle')
  })

  it('stop 断开 WebClient 并回到 idle', async () => {
    const s = useAnfFirstScreen()
    await s.stop()
    expect(mocks.initWebClient).toHaveBeenCalledWith(undefined, undefined, undefined)
    expect(s.status.value).toBe('idle')
  })

  it('stop 断开 WebClient 后删除所有运行实例并回 idle', async () => {
    mocks.listNetworkInstanceIds.mockResolvedValue({ running_inst_ids: ['i1', 'i2'], disabled_inst_ids: [] })
    const s = useAnfFirstScreen()
    await s.stop()
    expect(mocks.initWebClient).toHaveBeenCalledWith(undefined, undefined, undefined)
    expect(mocks.deleteNetworkInstance).toHaveBeenCalledWith('i1')
    expect(mocks.deleteNetworkInstance).toHaveBeenCalledWith('i2')
    expect(s.status.value).toBe('idle')
  })

  it('stop 删除实例失败不阻断，仍回 idle', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    mocks.listNetworkInstanceIds.mockResolvedValue({ running_inst_ids: ['i1'], disabled_inst_ids: [] })
    mocks.deleteNetworkInstance.mockRejectedValue(new Error('删除失败'))
    const s = useAnfFirstScreen()
    await s.stop()
    expect(s.status.value).toBe('idle')
    vi.restoreAllMocks()
  })

  it('start 连接失败标记 failed 并给出原因', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    s.machineId.value = 'm-1'
    mocks.initWebClient.mockRejectedValue(new Error('连不上'))
    await s.start()
    expect(s.status.value).toBe('failed')
    expect(s.errorMsg.value).toBe('连不上')
  })

  it('addProfile 新增档案并切换过去', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    s.addProfile()
    expect(s.profiles.value).toHaveLength(2)
    expect(s.activeIndex.value).toBe(1)
    expect(s.serverAddress.value).toBe('')
    await s.persist()
    const saved = mocks.anfSaveConfig.mock.calls[0][0] as Record<string, unknown>
    const profiles = saved.profiles as Array<Record<string, unknown>>
    expect(profiles[0].server_address).toBe('1.2.3.4:22020')
    expect(saved.active_profile_index).toBe(1)
  })

  it('switchProfile 切换并保留原档案字段', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    s.nickname.value = '办公室'
    s.addProfile()
    s.serverAddress.value = '5.6.7.8:22020'
    await s.switchProfile(0)
    expect(s.activeIndex.value).toBe(0)
    expect(s.serverAddress.value).toBe('1.2.3.4:22020')
    expect(s.nickname.value).toBe('办公室')
  })

  it('removeProfile 至少保留一个档案', async () => {
    const s = useAnfFirstScreen()
    s.addProfile()
    s.removeProfile(1)
    expect(s.profiles.value).toHaveLength(1)
    expect(s.activeIndex.value).toBe(0)
    // 只有 1 个时不可删除
    s.removeProfile(0)
    expect(s.profiles.value).toHaveLength(1)
  })
})
