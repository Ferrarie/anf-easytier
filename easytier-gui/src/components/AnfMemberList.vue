<script setup lang="ts">
import { onMounted, onBeforeUnmount, watch } from 'vue'
import { useMembers } from '~/composables/members'

const props = defineProps<{ instanceId?: string }>()
const { rows, loading, error, start, stop } = useMembers()

function fmtBytes(n: number): string {
  if (n <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  let v = n
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v.toFixed(v >= 10 ? 0 : 1)} ${units[i]}`
}

function costSeverity(cost: string): string {
  if (cost === 'p2p') return 'success'
  if (cost === 'Local') return 'primary'
  return 'secondary'
}

function costLabel(cost: string): string {
  return cost === 'p2p' ? '直连' : cost === 'Local' ? '本地' : '中转'
}

// 中心/服务器类节点友好化（参照 easytier-game member.vue）：PublicServer -> 服务器
function friendlyHostname(hostname: string): string {
  if ((hostname || '').toLowerCase().includes('publicserver')) {
    return (hostname || '').replace('PublicServer', '服务器')
  }
  return hostname || '-'
}

onMounted(() => {
  if (props.instanceId) start(props.instanceId)
})

watch(() => props.instanceId, (id) => {
  if (id) start(id)
  else stop()
})

onBeforeUnmount(stop)
</script>

<template>
  <div class="mt-3">
    <DataTable :value="rows" striped-rows size="small" :loading="loading" class="w-full text-sm">
      <template #empty>
        <div class="text-secondary text-center py-3">等待成员信息中…</div>
      </template>
      <Column field="hostname" header="成员名" sortable>
        <template #body="{ data }">
          <div class="font-medium">{{ friendlyHostname(data.hostname) }}</div>
          <div v-if="data.connections_addrs?.length" class="text-xs text-secondary break-all">
            {{ data.connections_addrs[0] }}
          </div>
        </template>
      </Column>
      <Column header="方式" sortable field="cost" style="width: 5rem">
        <template #body="{ data }">
          <Tag :severity="costSeverity(data.cost)">{{ costLabel(data.cost) }}</Tag>
        </template>
      </Column>
      <Column field="ipv4" header="虚拟 IP" sortable style="width: 8rem">
        <template #body="{ data }">{{ data.ipv4 || '-' }}</template>
      </Column>
      <Column field="lat_ms" header="延迟ms" sortable style="width: 6rem">
        <template #body="{ data }">{{ data.lat_ms >= 0 ? data.lat_ms : '-' }}</template>
      </Column>
      <Column field="loss_rate" header="丢包率" sortable style="width: 6rem">
        <template #body="{ data }">{{ data.loss_rate ? (data.loss_rate * 100).toFixed(2) + '%' : '0%' }}</template>
      </Column>
      <Column field="nat_type" header="NAT类型" sortable style="width: 7rem">
        <template #body="{ data }">{{ data.nat_type || '-' }}</template>
      </Column>
      <Column field="version" header="版本" sortable style="width: 8rem">
        <template #body="{ data }">{{ data.version || '-' }}</template>
      </Column>
      <Column field="tunnel_proto" header="隧道协议" sortable style="width: 7rem">
        <template #body="{ data }">{{ data.tunnel_proto || '-' }}</template>
      </Column>
      <Column header="接收" sortable field="rx_bytes" style="width: 8rem">
        <template #body="{ data }">{{ fmtBytes(data.rx_bytes) }}</template>
      </Column>
      <Column header="传输" sortable field="tx_bytes" style="width: 8rem">
        <template #body="{ data }">{{ fmtBytes(data.tx_bytes) }}</template>
      </Column>
    </DataTable>
    <Message v-if="error" severity="warn" :closable="false" class="mt-2 m-0">{{ error }}</Message>
  </div>
</template>
