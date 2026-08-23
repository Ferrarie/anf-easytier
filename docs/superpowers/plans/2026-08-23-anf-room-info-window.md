# ANF 房间信息（成员列表独立窗口）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 ANF 客户端首页的「成员列表」改造成「房间信息」独立窗口：成员名显示设备昵称、虚拟 IP 列显示网段内分配 IP，并显示网段 CIDR；窗口可缩放滚动、点按钮开合。

**Architecture:** 用 Tauri `WebviewWindow`（label=`member`）加载 `#/member` 路由；开合逻辑封装成 `composables/room_window.ts`（参照 easytier-game 的 `etWindows`），成员页 `pages/member.vue` 复用 `AnfMemberList.vue` 表格并新增 CIDR 说明。路由从 `createWebHistory` 切到 `createWebHashHistory` 以支持 `#/member` 深链。

**Tech Stack:** Vue 3 + unplugin-vue-router + PrimeVue + Tauri v2 (`@tauri-apps/api/webviewWindow` / `window`) + Vitest。

**Global Constraints**

- 命名口径按 `docs/anfagent-30/11-anf-naming-config-2026-08-22.md`：窗口标题「成员列表」，包名/进程名保持 `anf-easytier`。
- 成员名 = hostname（客户端已把昵称 display_name 作为 hostname 上报，见 `easytier-gui/src-tauri/src/lib.rs:583`）。
- 虚拟 IP = `route.ipv4_addr`（中心按网段分配，稳定）；CIDR 由 `info.my_node_info.virtual_ipv4`（`address.addr` + `network_length`）推导，不写死。
- 中心/服务器类节点友好化：hostname 含 `PublicServer` 显示「服务器」（参考 easytier-game `member.vue`）。
- 窗口几何：width≈880、height≈460、可缩放、可滚动、不最大化、`parent=主窗`、位于主窗右侧。
- 打开前置：仅当 status∈`connecting|pending|connected` 且存在 `lastInstanceId` 时才可开，否则按钮置灰并提示。
- Tauri v2 权限：新增 `core:webview:allow-create-webview-window`，capability 覆盖 `member` 窗口。
- 现有 GUI 测试与 vue-tsc 必须保持通过。

**Task 1: 推导网络 CIDR（纯函数，可单测）**

Files: Modify `easytier-gui/src/composables/members.ts`；Test `easytier-gui/src/composables/members.test.ts`。

Interfaces: Produces `export function networkCidr(info: unknown): string`，入参 `info.my_node_info.virtual_ipv4`，返回 `"10.11.74.0/24"`，无则 `""`。

Step 1 写失败测试：
```ts
import { networkCidr } from './members'
test('由 my_node_info.virtual_ipv4 推导网段 CIDR', () => {
  const info = { my_node_info: { virtual_ipv4: { address: { addr: 0x0A0B4A02 }, network_length: 24 } } }
  expect(networkCidr(info)).toBe('10.11.74.0/24')
})
test('无虚拟 IP 时返回空串', () => {
  expect(networkCidr({ my_node_info: { virtual_ipv4: undefined } })).toBe('')
})
```
Step 2 运行验证失败：`pnpm --dir easytier-gui vitest run src/composables/members.test.ts`（networkCidr 未定义）。
Step 3 最小实现：
```ts
function ipv4NumberToDotted(n: number): string {
  return [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255].join('.')
}
export function networkCidr(info: any): string {
  const v = info?.my_node_info?.virtual_ipv4
  const addr: number | undefined = v?.address?.addr
  const len: number | undefined = v?.network_length
  if (typeof addr !== 'number' || typeof len !== 'number' || len < 0 || len > 32) return ''
  const mask = len === 0 ? 0 : (0xffffffff << (32 - len)) >>> 0
  const base = (addr & mask) >>> 0
  return `${ipv4NumberToDotted(base)}/${len}`
}
```
Step 4 运行验证通过：同上命令，PASS 且既有用例 PASS。
Step 5 提交：`git commit -m "feat(anf): 成员列表推导网络 CIDR"`（含 members.ts 与 test）。

**Task 2: 路由切 hash，支持 `#/member`**

Files: Modify `easytier-gui/src/main.ts`。
把 `import { createRouter, createWebHistory } from 'vue-router/auto'` 改为 `createWebHashHistory`，`history: createWebHashHistory()`。
验证：`pnpm --dir easytier-gui build`（vue-tsc + vite 通过）。
提交：`git commit -m "feat(anf): 路由切 hash 以支持成员列表独立窗口"`。

**Task 3: 房间信息窗口开合决策（纯函数）**

Files: Create `easytier-gui/src/composables/room_window.ts`；Test `easytier-gui/src/composables/room_window.test.ts`。
Interfaces: `export type MemberWindowAction = 'create' | 'close' | 'show'`；`export function resolveMemberWindowAction(existing: { visible: boolean } | null): MemberWindowAction`；`export function canOpenMemberWindow(status: string, lastInstanceId?: string): boolean`。

Step 1 测试：
```ts
import { resolveMemberWindowAction, canOpenMemberWindow } from './room_window'
test('无窗口->create', () => expect(resolveMemberWindowAction(null)).toBe('create'))
test('可见->close', () => expect(resolveMemberWindowAction({ visible: true })).toBe('close'))
test('隐藏->show', () => expect(resolveMemberWindowAction({ visible: false })).toBe('show'))
test('运行中才可开', () => {
  expect(canOpenMemberWindow('connected', 'inst-1')).toBe(true)
  expect(canOpenMemberWindow('pending', 'inst-1')).toBe(true)
  expect(canOpenMemberWindow('failed', 'inst-1')).toBe(false)
  expect(canOpenMemberWindow('connected', undefined)).toBe(false)
})
```
Step 2 验证失败：`pnpm --dir easytier-gui vitest run src/composables/room_window.test.ts`。
Step 3 实现：
```ts
export type MemberWindowAction = 'create' | 'close' | 'show'
export function resolveMemberWindowAction(existing: { visible: boolean } | null): MemberWindowAction {
  if (!existing) return 'create'
  return existing.visible ? 'close' : 'show'
}
const OPENABLE = ['connecting', 'pending', 'connected']
export function canOpenMemberWindow(status: string, lastInstanceId?: string): boolean {
  return OPENABLE.includes(status) && !!lastInstanceId
}
```
Step 4 验证通过。Step 5 提交：`git commit -m "feat(anf): 房间信息窗口开合决策与前置条件"`。

**Task 4: 成员页 member.vue（滚动表格 + CIDR）**

Files: Create `easytier-gui/src/pages/member.vue`（自动注册 `/member`）。Consumes: `AnfMemberList.vue`、`useMembers`、`networkCidr`、`anfLoadConfig`、`collectNetworkInfo`。

实现：onMounted 中 `JSON.parse(await anfLoadConfig())` 取 `profiles[active_profile_index].last_instance_id`，`collectNetworkInfo` 拿 `info.my_node_info` 算 cidr，模板 `<div class="h-full w-full overflow-auto p-2">` 内先显示 `网段：{{ cidr }}`，再放 `<AnfMemberList :instance-id="instanceId" />`。
验证：`pnpm --dir easytier-gui build`。
提交：`git commit -m "feat(anf): 房间信息成员页（滚动+网段CIDR）"`。

**Task 5: 主屏接入「房间信息」按钮 + 开关窗口**

Files: Modify `easytier-gui/src/components/AnfFirstScreen.vue`；追加到 `easytier-gui/src/composables/room_window.ts`。
在 room_window.ts 追加 `toggleMemberWindow`：
```ts
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
export async function toggleMemberWindow(status: string, lastInstanceId?: string): Promise<void> {
  if (!canOpenMemberWindow(status, lastInstanceId)) return
  const existing = await WebviewWindow.getByLabel('member')
  const action = resolveMemberWindowAction(existing ? { visible: await existing.isVisible() } : null)
  if (action === 'close') { await existing?.close(); return }
  if (action === 'show') { await existing?.show(); await existing?.setFocus(); return }
  const app = getCurrentWindow()
  const factor = await app.scaleFactor()
  const pos = await app.outerPosition()
  const logical = new PhysicalPosition(pos.x + Math.ceil(345 * factor), pos.y).toLogical(factor)
  new WebviewWindow('member', {
    title: '成员列表', width: 880, height: 460, url: '/#/member',
    parent: app, x: logical.x, y: logical.y,
    closable: true, resizable: true, decorations: true,
    maximizable: false, minimizable: false,
  })
}
```
AnfFirstScreen.vue：删除 `membersOpen` ref 与内联 `<AnfMemberList>`；把「成员列表」按钮替换为：
```vue
<Button text size="small" icon="pi pi-users" label="房间信息" class="p-0"
  :disabled="!canOpenMemberWindow(status, lastInstanceId)" @click="toggleMemberWindow(status, lastInstanceId)" />
```
顶部 `import { toggleMemberWindow, canOpenMemberWindow } from '~/composables/room_window'`。
验证：`pnpm --dir easytier-gui build`。提交：`git commit -m "feat(anf): 首页改房间信息按钮，开合成员独立窗口"`。

**Task 6: Tauri 权限与 capability**

Files: Modify `easytier-gui/src-tauri/capabilities/migrated.json`，`windows` 加 `"member"`，permissions 加 `core:webview:allow-create-webview-window` 与 `core:webview:default`。
验证：`pnpm --dir easytier-gui tauri build --no-bundle`。提交：`git commit -m "feat(anf): 授权创建成员列表窗口"`。

**Task 7: 全量回归 + 手工冒烟**

- `pnpm --dir easytier-gui vitest run` 全部 PASS。
- `pnpm --dir easytier-gui tauri build --no-bundle` 通过。
- 手工：启动 exe，未启动时「房间信息」置灰；启动后点击打开独立窗口（成员列表、网段 CIDR、成员昵称/虚拟 IP、可滚动缩放）；再点关闭。

**Self-Review**

- Spec 覆盖：①成员名=昵称（hostname 已有）+中心友好名（member.vue 展示层）→ Task 4；②虚拟 IP 列显示分配 IP（`route.ipv4_addr`）+CIDR → Task 1/4；③房间信息按钮+独立窗口+开合+滚动 → Task 2/3/5/6。
- 占位符：无 TBD/TODO。
- 类型一致性：`networkCidr`/`canOpenMemberWindow`/`resolveMemberWindowAction`/`toggleMemberWindow` 命名一致。
