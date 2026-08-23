# ANF 客户端成员列表（Member List）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 ANF 客户端 GUI 增加一个"成员列表"只读看板，展示同一逻辑网络（同一 network instance / 同一 mesh）下所有在线成员的连接状态（成员名、方式、虚拟 IP、延迟、丢包率、NAT 类型、版本、隧道协议、接收/传输字节），每 1 秒刷新。

**Architecture:** 复用 easytier-core 已有的 `collect_network_info` RPC（返回 `NetworkInstanceRunningInfo`，内含 `routes` + `peers` + `my_node_info`），在 Rust Tauri 侧新增一个 `get_members` 命令；前端在 `AnfFirstScreen` 里新增一个可展开的 `AnfMemberList.vue` 子组件，用 PrimeVue `DataTable` 渲染，`setInterval` 每 1 秒轮询。**不复用 easytier-game 的外部 spawn `easytier-cli` 方式**，避免子进程/路径/权限问题。

**Tech Stack:** Tauri 2（Rust + `easytier-core`）、Vue 3 + TypeScript、PrimeVue（`DataTable`）、`collect_network_info` RPC。

## Global Constraints

- 产物/版本口径沿用 `2.6.4`；不引入 element-plus 新依赖，统一用 PrimeVue。
- 数据源只用 `collect_network_info`（单次聚合 route+peer+node），**不要**外调 `easytier-cli`，**不要**新增 proxy/子网代理展示。
- 只读看板：不提供"踢下线/审批/删除"等管理操作（那些归 web 后台），不点开成员详情弹窗。
- 刷新为 1 秒轮询（用户指定），仅当"已连接且有运行实例"时才轮询；未连接/无实例显示"等待成员信息中…"。
- 连接方式映射（与截图语义一致，中文）：`Local`→本地、`p2p`→直连、`relay`→中转；直连用绿色 Tag、中转用灰色 Tag。
- 现有 `AnfFirstScreen.vue` 的布局 / 命名 / 自动保存行为不得破坏。
- **参考实现仓库**：`%TEMP%\easytier-game-ref`（git clone 自 https://github.com/EasyTier/EasytierGame，保留不删除）。
  参考它的 `pages/member.vue`（成员表结构、方式/NAT/字节的显示映射）、`src-tauri/src/lib.rs` 的
  `get_members_by_cli` / `get_members_connections_cli` / `get_members_proxy`（数据获取思路）。
  但**我们不走它那种 spawn `easytier-cli` 子进程**，而是复用本仓库已有的 `collect_network_info` RPC。
- **数据源校准结论**：`collect_network_info` 返回 `info.map[instance_id]`（即 `NetworkInstanceRunningInfo`），
  一次性含 `routes`（hostname/ipv4_addr/next_hop_peer_id/version/feature_flag）+ `peers`
  （conns: tunnel/stats/loss_rate/directly_connected_conns）+ `my_node_info`。前端 `backend.collectNetworkInfo()`
  已封装此命令，**无需新增 Rust 命令**——修正后删去原 Task 1。
- **vitest 入口**：项目主 `vite.config.ts` 在 vitest 下会因 unplugin/devtools 挂起，测试命令一律加
  `--config vitest.minimal.config.ts`（该文件仅含 vue + `~/` alias，已建好）。`assets` 下不重新加载主 config。

---

## File Structure

- **Create `easytier-gui/src/composables/members.ts`** — 数据获取 + 类型归一化的 composable：复用 `backend.collectNetworkInfo()` 拿到 `NetworkInstanceRunningInfo`，归一化为一行一个成员的 `MemberRow[]`，并提供 `start/stop` 轮询。
- **Create `easytier-gui/src/components/AnfMemberList.vue`** — 成员列表展示组件（PrimeVue `DataTable`），消费 `useMembers()`，对外暴露 props 控制是否轮询。
- **Create `easytier-gui/src/composables/members.test.ts`** — `members.ts` 的 vitest 单测（归一化逻辑，无 Tauri 依赖）。
- **Modify `easytier-gui/src-tauri/src/lib.rs`** — 新增 `#[tauri::command] get_members`，调用 `handle_collect_network_info`，返回归一化 JSON；并注册进 `generate_handler!`。
- **Modify `easytier-gui/src/components/AnfFirstScreen.vue`** — 在"高级"下方/旁边加一个"成员列表"入口，展开时挂载 `AnfMemberList`。
- 无需改 `backend.ts`：直接复用已有 `collectNetworkInfo()`。

## Field Mapping (从 `NetworkInstanceRunningInfo`)

`collect_network_info` 返回 `info.map[instance_id]`，其字段映射到一行成员：

| 截图列 | 来源 | 归一化字段 |
| --- | --- | --- |
| 成员名 | `routes[].hostname` | `hostname` |
| 方式 | `peer.directly_connected_conns` 非空 / `route.next_hop_peer_id == my_peer_id` / `route.cost` | `cost`（'p2p'/'relay'/'Local'） |
| 虚拟网 IP | `routes[].ipv4_addr.address` | `ipv4` |
| 延迟 ms | `conns[].stats.latency_us / 1000`（取最优连接） | `lat_ms` |
| 丢包率 | `conns[].loss_rate` | `loss_rate` |
| NAT 类型 | `routes[].stun_info.udp_nat_type`（数字→枚举名） | `nat_type` |
| 版本 | `routes[].version` | `version` |
| 隧道协议 | `conns[].tunnel` 的 scheme/proto | `tunnel_proto` |
| 接收 | `conns[].stats.rx_bytes` | `rx_bytes` |
| 传输 | `conns[].stats.tx_bytes` | `tx_bytes` |
| 连接地址 | `conns[].tunnel.remote_addr` | `connections_addrs[]` |

- 本机行：`my_node_info.peer_id` 对应的路由/peer；`cost='Local'`。
- 中转判定：某 peer 的路由 `next_hop_peer_id != 自身` 且无 `directly_connected_conns` → `cost='relay'`。
- 直连判定：存在任一非空 `directly_connected_conns`（或 peer_id 就是直连） → `cost='p2p'`。
- 端口/子网清洗：`ipv4_addr.address` 可能带 `/len`，前端 `split('/')[0]` 取纯 IP。

---

### Task 1: 前端 `members.ts` composable + 类型

**Files:**
- Create: `easytier-gui/src/composables/members.ts`
- Test: `easytier-gui/src/composables/members.test.ts`

**Interfaces:**
- Consumes: `collectNetworkInfo(instanceId)`（已存在于 `backend.ts`，返回 `Api.CollectNetworkInfoResponse`，取 `info.map[instanceId]` 即 `NetworkInstanceRunningInfo`）。
- Produces: `useMembers()` 返回 `{ rows, loading, error, start(instanceId), stop() }`；`MemberRow` 类型导出。

- [ ] **Step 1: 写失败测试** `members.test.ts`

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { normalizeMembers } from './members'

describe('normalizeMembers', () => {
  it('映射 route+peer 为成员行，清洗 IP 端口/子网', () => {
    const info: any = {
      my_node_info: { peer_id: 1 },
      routes: [{
        peer_id: 1, hostname: 'soso', ipv4_addr: { address: '10.0.0.3/24' },
        stun_info: { udp_nat_type: 4 }, version: '2.4.5',
        next_hop_peer_id: 1,
      }, {
        peer_id: 2, hostname: 'bridge', ipv4_addr: { address: '10.200.126.1/24' },
        stun_info: { udp_nat_type: 3 }, version: '2.4.5',
        next_hop_peer_id: 2,
      }],
      peers: [{ peer_id: 2, conns: [{ stats: { latency_us: 23000, rx_bytes: 100, tx_bytes: 200 }, loss_rate: 0.0, tunnel: { remote_addr: { url: 'tcp://203.0.113.33:10791' } } }], directly_connected_conns: ['x'] }],
    }
    const rows = normalizeMembers(info)
    expect(rows[1].ipv4).toBe('10.200.126.1')
    expect(rows[1].cost).toBe('p2p')
    expect(rows[0].cost).toBe('Local')
    expect(rows[1].lat_ms).toBe(23)
  })
})
```

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm --filter anf-easytier exec vitest run --config vitest.minimal.config.ts src/composables/members.test.ts`
Expected: FAIL（`normalizeMembers` 未定义）

- [ ] **Step 3: 写实现** `members.ts`

```ts
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

const NAT_MAP: Record<string, string> = {
  Unknown: '未知', OpenInternet: 'nat0', NoPat: 'nat0-nopat',
  FullCone: 'nat1', Restricted: 'nat2', PortRestricted: 'nat3',
  Symmetric: 'nat4',
}

export function normalizeMembers(info: any): MemberRow[] {
  if (!info?.routes) return []
  const myPeerId = info.my_node_info?.peer_id
  return info.routes.map((route: any) => {
    const peer = info.peers?.find((p: any) => p.peer_id === route.peer_id)
    const conns = peer?.conns ?? []
    const best = [...conns].sort((a: any, b: any) =>
      (a.stats?.latency_us ?? Number.MAX_SAFE_INTEGER) - (b.stats?.latency_us ?? Number.MAX_SAFE_INTEGER))[0]
    const directly = !!peer && (peer.directly_connected_conns?.length ?? 0) > 0
    const cost = myPeerId === route.peer_id ? 'Local'
      : directly && route.next_hop_peer_id === route.peer_id ? 'p2p'
      : (route.next_hop_peer_id ?? 0) !== 0 && route.next_hop_peer_id !== route.peer_id ? 'relay'
      : directly ? 'p2p' : 'relay'
    // frontend-lib 会把 Ipv4Addr 归一化为字符串（如 "10.0.0.3"）；若为数字则格式化。
    const ipv4Addr = route.ipv4_addr?.address
    const ipv4 = typeof ipv4Addr === 'string'
      ? ipv4Addr.split('/')[0]
      : (ipv4Addr && typeof ipv4Addr.addr === 'number')
        ? [ipv4Addr.addr >>> 24, (ipv4Addr.addr >>> 16) & 255, (ipv4Addr.addr >>> 8) & 255, ipv4Addr.addr & 255].join('.')
        : ''
    const tunnel = best?.tunnel
    return {
      peer_id: route.peer_id,
      hostname: route.hostname ?? '',
      cost,
      ipv4,
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
    loading.value = true
    try {
      const resp = await collectNetworkInfo(instanceId)
      const info = resp?.info?.map?.[instanceId]
      rows.value = normalizeMembers(info)
      error.value = undefined
    } catch (e: any) {
      error.value = e instanceof Error ? e.message : String(e)
    } finally {
      loading.value = false
    }
  }

  function start(instanceId: string) {
    stop()
    void fetch(instanceId)
    timer = setInterval(() => void fetch(instanceId), 1000)
  }

  function stop() {
    if (timer) { clearInterval(timer); timer = null }
  }

  return { rows, loading, error, start, stop, fetch }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `pnpm --filter anf-easytier exec vitest run --config vitest.minimal.config.ts src/composables/members.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add easytier-gui/src/composables/members.ts easytier-gui/src/composables/members.test.ts
git commit -m "feat(anf): add members composable reusing collect_network_info"
```

---

### Task 3: `AnfMemberList.vue` 展示组件

**Files:**
- Create: `easytier-gui/src/components/AnfMemberList.vue`

**Interfaces:**
- Consumes: `useMembers()` 的 `{ rows, loading, error, start, stop }`；外部传入 `instanceId`。
- Produces: props `instanceId?: string`；无对外事件。

- [ ] **Step 1: 写组件**（PrimeVue `DataTable`）

```vue
<script setup lang="ts">
import { onMounted, onBeforeUnmount, watch } from 'vue'
import { useMembers } from '~/composables/members'

const props = defineProps<{ instanceId?: string }>()
const { rows, loading, error, start, stop } = useMembers()

function fmtBytes(n: number): string {
  if (n <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0, v = n
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++ }
  return `${v.toFixed(v >= 10 ? 0 : 1)} ${units[i]}`
}

function costType(cost: string): string {
  if (cost === 'p2p') return 'success'
  if (cost === 'Local') return 'primary'
  return 'secondary'
}

function costLabel(cost: string): string {
  return cost === 'p2p' ? '直连' : cost === 'Local' ? '本地' : '中转'
}

onMounted(() => { if (props.instanceId) start(props.instanceId) })
watch(() => props.instanceId, (id) => { if (id) start(id); else stop() })
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
          <div class="font-medium">{{ data.hostname || '-' }}</div>
          <div v-if="data.connections_addrs?.length" class="text-xs text-secondary break-all">
            {{ data.connections_addrs[0] }}
          </div>
        </template>
      </Column>
      <Column header="方式" sortable field="cost" style="width: 5rem">
        <template #body="{ data }">
          <Tag :severity="costType(data.cost)">{{ costLabel(data.cost) }}</Tag>
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
```

- [ ] **Step 2: 确认 PrimeVue `DataTable`/`Tag`/`Message` 已自动导入**（本项目用 `unplugin-vue-components`，无需手动 import；若未自动导入，在脚本加 `import { DataTable, Column, Tag, Message } from 'primevue'`）。

- [ ] **Step 3: 编译验证**

Run: `pnpm --filter anf-easytier exec vue-tsc --noEmit -p tsconfig.json`
Expected: 通过

- [ ] **Step 4: Commit**

```bash
git add easytier-gui/src/components/AnfMemberList.vue
git commit -m "feat(anf): add member list table component"
```

---

### Task 4: 接入 `AnfFirstScreen`（成员列表入口）

**Files:**
- Modify: `easytier-gui/src/components/AnfFirstScreen.vue`

**Interfaces:**
- Consumes: `useAnfFirstScreen()` 已有的 `lastInstanceId`；`AnfMemberList` 组件。
- Produces: 无对外接口变化。

- [ ] **Step 1: 加"成员列表"入口与挂载**：在"高级"展开区之后新增 `membersOpen` 状态与按钮。

```vue
<script setup lang="ts">
// 在已有 onMounted/useAnfFirstScreen 逻辑后加
const membersOpen = ref(false)
</script>

<template>
  <!-- 放在 "高级" 那块 </div> 之后 -->
  <div class="border-t pt-3">
    <Button text size="small" icon="pi pi-users" label="成员列表" class="p-0"
      @click="membersOpen = !membersOpen" />
    <div v-if="membersOpen" class="mt-3">
      <AnfMemberList :instance-id="lastInstanceId" />
    </div>
  </div>
</template>
```

- [ ] **Step 2: 导入组件**：`import AnfMemberList from '~/components/AnfMemberList.vue'`。

- [ ] **Step 3: 编译验证**

Run: `pnpm --filter anf-easytier exec vue-tsc --noEmit -p tsconfig.json`
Expected: 通过

- [ ] **Step 4: Commit**

```bash
git add easytier-gui/src/components/AnfFirstScreen.vue
git commit -m "feat(anf): add member list entry in ANF quick connect screen"
```

---

## Self-Review（作者自查）

- 数据源：用一条 `collect_network_info` 聚合，不 spawn CLI——符合 Q9 决策。
- 无 proxy/子网展示——符合 Q10。
- PrimeVue `DataTable`——符合 Q11。
- 只读看板、无详情弹窗——符合 Q12。
- 1 秒轮询——符合 Q13。
- 空态"等待成员信息中…"——符合 Q14。
- 独立 `AnfMemberList.vue` + 首页入口——符合 Q15。
- 每任务有独立可测交付；归一化逻辑有单测；字段映射覆盖截图全部列。
- 风险点已标注：`TunnelInfo`/`StunInfo` 的字段名需以 `easytier-proto` 生成类型为准（Task 1 Step 3 备注），前端 NAT 枚举值可能需按实际返回串调整（`NAT_MAP`）。
