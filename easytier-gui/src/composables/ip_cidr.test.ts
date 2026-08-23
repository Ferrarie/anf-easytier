import { describe, it, expect } from 'vitest'
import { networkCidr } from './ip_cidr'

describe('networkCidr', () => {
  it('由 my_node_info.virtual_ipv4 推导网段 CIDR（/24）', () => {
    // 10.11.74.2 / 24 -> 10.11.74.0/24
    const info: any = { my_node_info: { virtual_ipv4: { address: { addr: 0x0A0B4A02 }, network_length: 24 } } }
    expect(networkCidr(info)).toBe('10.11.74.0/24')
  })

  it('无虚拟 IP 时返回空串', () => {
    expect(networkCidr({ my_node_info: {} })).toBe('')
  })

  it('短前缀仍正确掩码（/16）', () => {
    // 10.11.74.2 / 16 -> 10.11.0.0/16
    const info: any = { my_node_info: { virtual_ipv4: { address: { addr: 0x0A0B4A02 }, network_length: 16 } } }
    expect(networkCidr(info)).toBe('10.11.0.0/16')
  })
})
