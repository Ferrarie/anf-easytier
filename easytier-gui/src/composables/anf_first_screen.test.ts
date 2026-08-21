import { describe, expect, it, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  anfLoadConfig: vi.fn(),
  anfSaveConfig: vi.fn(),
  anfGetMachineId: vi.fn(),
  anfNormalizeAddress: vi.fn(),
}))

vi.mock('./backend', () => mocks)

import { useAnfFirstScreen } from './anf_first_screen'

describe('useAnfFirstScreen', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.anfGetMachineId.mockResolvedValue('m-1')
    mocks.anfNormalizeAddress.mockResolvedValue('tcp://10.0.0.1:22020')
  })

  it('init 载入配置并补齐机器 ID', async () => {
    mocks.anfLoadConfig.mockResolvedValue(JSON.stringify({
      schema_version: 1,
      machine_id: 'm-existing',
      server_address: '10.0.0.1:22020',
      network_name: 'anf-m3',
      invite_code: 'INV-1',
      invite_status: 'pending',
    }))
    const s = useAnfFirstScreen()
    await s.init()
    expect(s.serverAddress.value).toBe('10.0.0.1:22020')
    expect(s.machineId.value).toBe('m-existing')
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
    s.inviteCode.value = 'INV-9'
    s.serverAddress.value = '1.2.3.4:22020'
    s.networkName.value = 'anf-m3'
    const path = await s.persist()
    expect(path).toBe('C:/app/config.toml')
    const saved = mocks.anfSaveConfig.mock.calls[0][0] as Record<string, unknown>
    expect(saved.server_address).toBe('1.2.3.4:22020')
    // 配置结构不含网络密钥字段
    expect(JSON.stringify(saved)).not.toMatch(/secret|password|network_key/i)
  })

  it('start 状态机走到 pending（占位）', async () => {
    const s = useAnfFirstScreen()
    s.serverAddress.value = '1.2.3.4:22020'
    await s.start()
    expect(s.status.value).toBe('pending')
  })
})
