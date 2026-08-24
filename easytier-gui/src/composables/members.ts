import { ref } from 'vue'
import { collectNetworkInfo } from './backend'

export interface MemberRow {
  peer_id: number
  hostname: string
  cost: string
  ipv4: string
  lat_ms: number
  loss_rate: number
  nat_type: string
  version: string
  tunnel_proto: string
  rx_bytes: number
  tx_bytes: number
  connections_addrs: string[]
}

// 与 easytier-game natMaps 对齐：数字/字符串枚举名 → 中文 NAT 描述
const NAT_MAP: Record<string, string> = {
  Unknown: '未知',
  OpenInternet: 'nat0',
  NoPat: 'nat0-nopat',
  FullCone: 'nat1',
  Restricted: 'nat2',
  PortRestricted: 'nat3',
  Symmetric: 'nat4',
  SymmetricEasyInc: 'nat4-easyinc',
  SymmetricEasyDec: 'nat4-easydec',
  SymmetricUdpFirewall: 'nat4-udpfirewall',
}

function formatIpv4(addr: any): string {
  if (typeof addr === 'string') {
    return addr.split('/')[0]
  }
  if (addr && typeof addr.addr === 'number') {
    const n = addr.addr >>> 0
    return [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255].join('.')
  }
  return ''
}

export function normalizeMembers(info: any): MemberRow[] {
  if (!info?.routes)
    return []
  const myPeerId = info.my_node_info?.peer_id
  return info.routes.map((route: any) => {
    const peer = info.peers?.find((p: any) => p.peer_id === route.peer_id)
    const conns = peer?.conns ?? []
    const best = [...conns].sort((a: any, b: any) =>
      (a.stats?.latency_us ?? Number.MAX_SAFE_INTEGER) - (b.stats?.latency_us ?? Number.MAX_SAFE_INTEGER))[0]
    const directly = !!peer && (peer.directly_connected_conns?.length ?? 0) > 0
    const cost = myPeerId === route.peer_id
      ? 'Local'
      : directly && route.next_hop_peer_id === route.peer_id
        ? 'p2p'
        : (route.next_hop_peer_id ?? 0) !== 0 && route.next_hop_peer_id !== route.peer_id
            ? 'relay'
            : directly ? 'p2p' : 'relay'
    const tunnel = best?.tunnel
    return {
      peer_id: route.peer_id,
      hostname: route.hostname ?? '',
      cost,
      ipv4: formatIpv4(route.ipv4_addr?.address),
      lat_ms: best?.stats?.latency_us ? Math.round(best.stats.latency_us / 1000) : -1,
      loss_rate: best?.loss_rate ?? 0,
      nat_type: NAT_MAP[String(route.stun_info?.udp_nat_type ?? '')] ?? String(route.stun_info?.udp_nat_type ?? '未知'),
      version: route.version ?? '',
      tunnel_proto: tunnel?.tunnel_type ?? '',
      rx_bytes: best?.stats?.rx_bytes ?? 0,
      tx_bytes: best?.stats?.tx_bytes ?? 0,
      connections_addrs: tunnel?.remote_addr?.url ? [tunnel.remote_addr.url] : [],
    }
  })
}

export function useMembers() {
  const rows = ref<MemberRow[]>([])
  const loading = ref(false)
  const error = ref<string | undefined>(undefined)
  let timer: ReturnType<typeof setInterval> | null = null

  async function fetch(instanceId: string) {
    // 仅首屏（列表为空）显示 loading；后台刷新静默更新，避免 DataTable 遮罩闪烁。
    if (rows.value.length === 0) {
      loading.value = true
    }
    try {
      const resp = await collectNetworkInfo(instanceId)
      const info = resp?.info?.map?.[instanceId]
      rows.value = normalizeMembers(info)
      error.value = undefined
    }
    catch (e: any) {
      error.value = e instanceof Error ? e.message : String(e)
    }
    finally {
      loading.value = false
    }
  }

  function start(instanceId: string) {
    stop()
    void fetch(instanceId)
    timer = setInterval(() => void fetch(instanceId), 1000)
  }

  function stop() {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  }

  return { rows, loading, error, start, stop, fetch }
}
