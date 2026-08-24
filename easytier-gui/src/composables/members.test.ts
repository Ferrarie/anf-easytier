import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  collectNetworkInfo: vi.fn(),
}))

vi.mock('./backend', () => mocks)

import { useMembers, normalizeMembers } from './members'

describe('normalizeMembers', () => {
  it('映射 route+peer 为成员行，cost 判定本地/直连/中转', () => {
    const info: any = {
      my_node_info: { peer_id: 1 },
      routes: [
        {
          peer_id: 1,
          hostname: 'soso',
          ipv4_addr: { address: '10.0.0.3/24' },
          stun_info: { udp_nat_type: 'Symmetric' },
          version: '2.4.5',
          next_hop_peer_id: 1,
        },
        {
          peer_id: 2,
          hostname: 'bridge',
          ipv4_addr: { address: '10.200.126.1/24' },
          stun_info: { udp_nat_type: 'PortRestricted' },
          version: '2.4.5',
          next_hop_peer_id: 2,
        },
        {
          peer_id: 3,
          hostname: 'docker',
          ipv4_addr: { address: '10.200.126.4/24' },
          stun_info: { udp_nat_type: 'PortRestricted' },
          version: '2.4.5',
          next_hop_peer_id: 0,
        },
      ],
      peers: [
        {
          peer_id: 2,
          conns: [{
            stats: { latency_us: 23000, rx_bytes: 100, tx_bytes: 200 },
            loss_rate: 0.0,
            tunnel: { tunnel_type: 'tcp', remote_addr: { url: 'tcp://203.0.113.33:10791' } },
          }],
          directly_connected_conns: ['x'],
        },
      ],
    }

    const rows = normalizeMembers(info)

    expect(rows).toHaveLength(3)
    expect(rows[0].cost).toBe('Local')
    expect(rows[0].ipv4).toBe('10.0.0.3')
    expect(rows[0].nat_type).toBe('nat4')
    expect(rows[1].cost).toBe('p2p')
    expect(rows[1].ipv4).toBe('10.200.126.1')
    expect(rows[1].lat_ms).toBe(23)
    expect(rows[1].tunnel_proto).toBe('tcp')
    expect(rows[1].connections_addrs[0]).toBe('tcp://203.0.113.33:10791')
    expect(rows[2].cost).toBe('relay')
  })

  it('routes 为空时返回空数组', () => {
    expect(normalizeMembers({})).toEqual([])
  })

  it('支持 Ipv4Addr 为数字 addr 时格式化为点分十进制', () => {
    const info: any = {
      my_node_info: { peer_id: 1 },
      routes: [{
        peer_id: 1,
        hostname: 'local',
        ipv4_addr: { address: { addr: 0x0a7e7e03 } }, // 10.0.0.3
        stun_info: { udp_nat_type: 'FullCone' },
        version: '2.6.4',
        next_hop_peer_id: 1,
      }],
      peers: [],
    }
    const rows = normalizeMembers(info)
    expect(rows[0].ipv4).toBe('10.0.0.3')
    expect(rows[0].nat_type).toBe('nat1')
  })
})

describe('useMembers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('首屏（列表为空）加载期间置 loading，结束后回 false', async () => {
    let resolveFetch!: (v: unknown) => void
    mocks.collectNetworkInfo.mockReturnValue(new Promise((r) => { resolveFetch = r }))
    const m = useMembers()
    const p = m.fetch('i1')
    expect(m.loading.value).toBe(true)
    resolveFetch({ info: { map: { i1: { routes: [] } } } })
    await p
    expect(m.loading.value).toBe(false)
  })

  it('已有数据时后台刷新不置 loading，保持静默更新', async () => {
    const infoWithMember = {
      info: {
        map: {
          i1: {
            my_node_info: { peer_id: 1 },
            routes: [{ peer_id: 1, hostname: 'local' }],
            peers: [],
          },
        },
      },
    }
    mocks.collectNetworkInfo.mockResolvedValue(infoWithMember)
    const m = useMembers()
    await m.fetch('i1')
    expect(m.rows.value).toHaveLength(1)

    let resolveFetch!: (v: unknown) => void
    mocks.collectNetworkInfo.mockReturnValue(new Promise((r) => { resolveFetch = r }))
    const p = m.fetch('i1')
    expect(m.loading.value).toBe(false)
    resolveFetch(infoWithMember)
    await p
    expect(m.loading.value).toBe(false)
  })
})
