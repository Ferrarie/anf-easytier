import { describe, expect, it, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  anfLoadConfig: vi.fn(),
  anfSaveConfig: vi.fn(),
  anfGetMachineId: vi.fn(),
  anfNormalizeAddress: vi.fn(),
  initWebClient: vi.fn(),
  isWebClientConnected: vi.fn(),
  listNetworkInstanceIds: vi.fn(),
}))

vi.mock('./backend', () => mocks)

import { useAnfFirstScreen } from './anf_first_screen'

describe('useAnfFirstScreen', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.anfGetMachineId.mockResolvedValue('m-1')
    mocks.anfNormalizeAddress.mockResolvedValue('tcp://10.0.0.1:22020')
    mocks.initWebClient.mockResolvedValue(undefined)
    mocks.isWebClientConnected.mockResolvedValue(false)
    mocks.listNetworkInstanceIds.mockResolvedValue({ running_inst_ids: [], disabled_inst_ids: [] })
  })

  it('init 载入配置并补齐机器 ID', async () => {
    mocks.anfLoadConfig.mockResolvedValue(JSON.stringify({
      schema_version: 1,
      machine_id: 'm-existing',
      server_address: '10.0.0.1:22020',
      nickname: '小白-办公',
    }))
    const s = useAnfFirstScreen()
    await s.init()
    expect(s.serverAddress.value).toBe('10.0.0.1:22020')
    expect(s.machineId.value).toBe('m-existing')
    expect(s.nickname.value).toBe('小白-办公')
    expect(mocks.anfGetMachineId).not.toHaveBeenCalled()
  })

  it('init 无配置时生成机器 ID', async () => {
    mocks.anfLoadConfig.mockResolvedValue('{}')
    const s = useAnfFirstScreen()
    await s.init()
    expect(s.machineId.value).toBe('m-1')
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

  it('persist 保存非机密配置', async () => {
    mocks.anfSaveConfig.mockResolvedValue('C:/app/config.toml')
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    s.nickname.value = '办公室'
    const path = await s.persist()
    expect(path).toBe('C:/app/config.toml')
    const saved = mocks.anfSaveConfig.mock.calls[0][0] as Record<string, unknown>
    expect(saved.server_address).toBe('1.2.3.4:22020')
    expect(saved.nickname).toBe('办公室')
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
    // normalizeAddress 内部会 set 错误消息，这里直接断言 status 回到 idle
    expect(s.status.value).toBe('idle')
  })

  it('stop 断开 WebClient 并回到 idle', async () => {
    const s = useAnfFirstScreen()
    await s.stop()
    expect(mocks.initWebClient).toHaveBeenCalledWith(undefined, undefined, undefined)
    expect(s.status.value).toBe('idle')
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

  it('stop 后再 start 仍能重新连接', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    s.machineId.value = 'm-1'
    await s.stop()
    await s.start()
    expect(mocks.initWebClient).toHaveBeenCalledWith('tcp://10.0.0.1:22020', 'm-1', undefined)
    s.cleanup()
  })
})
