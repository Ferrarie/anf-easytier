// 处理虚拟 IP / 网段 CIDR 的纯函数（无 Tauri / frontend-lib 依赖，便于单测）。

function ipv4NumberToDotted(n: number): string {
  return [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255].join('.')
}

/**
 * 根据本机节点上报的 virtual_ipv4（address.addr + network_length）推导网络网段 CIDR。
 * 例：10.11.74.2/24 → "10.11.74.0/24"；无虚拟 IP 返回 ""。
 */
export function networkCidr(info: any): string {
  const v = info?.my_node_info?.virtual_ipv4
  const addr: number | undefined = v?.address?.addr
  const len: number | undefined = v?.network_length
  if (typeof addr !== 'number' || typeof len !== 'number' || len < 0 || len > 32) return ''
  const mask = len === 0 ? 0 : (0xffffffff << (32 - len)) >>> 0
  const base = (addr & mask) >>> 0
  return `${ipv4NumberToDotted(base)}/${len}`
}
