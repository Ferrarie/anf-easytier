# GUI 停止连接完全断开 + 成员窗口刷新去闪烁 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 GUI 两个问题——点击停止后真正断开（虚拟网卡消失、服务器 web 不再显示在线）；成员列表窗口刷新不再闪烁。

**Architecture:** 纯前端 composable 改动。`anf_first_screen.ts` 的 `stop()` 在断开 config-server 后枚举并删除所有运行实例；`members.ts` 的 `fetch()` 仅首屏（列表为空）置 loading。两个改动都有对应 vitest 单测。

**Tech Stack:** Vue 3 + TypeScript + Tauri 2 + PrimeVue 4 + Vitest 2。

## Global Constraints

- 不修改 Rust 侧 `init_web_client(None)` 语义。
- 保留 `last_instance_id` 自动重连语义（停止后不清除）。
- 成员窗口轮询周期保持 1 秒。
- Windows 环境：命令一律用 PowerShell（pwsh）执行。
- 每个任务独立提交，提交信息遵循仓库现有风格（`fix(anf): ...` / `test(anf): ...`）。

---

### Task 1: 成员列表仅首屏显示 loading（修复刷新闪烁）

**Files:**
- Modify: `easytier-gui/src/composables/members.ts:76-96`（`fetch` 函数）
- Test: `easytier-gui/src/composables/members.test.ts`

**Interfaces:**
- Consumes: `useMembers()` 现有导出 `{ rows, loading, error, start, stop, fetch }` 不变。
- Produces: `fetch(instanceId)` 行为变化——已有数据时后台刷新不再置 `loading = true`。

- [ ] **Step 1: 写失败测试**

在 `members.test.ts` 顶部加 backend mock（保留现有 `normalizeMembers` 用例）：

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  collectNetworkInfo: vi.fn(),
}))

vi.mock('./backend', () => mocks)

import { useMembers, normalizeMembers } from './members'
```

在文件末尾追加：

```ts
describe('useMembers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('已有数据时后台刷新不置 loading，保持静默更新', async () => {
    mocks.collectNetworkInfo.mockResolvedValue({ info: { map: { i1: { routes: [] } } } })
    const m = useMembers()
    await m.fetch('i1')
    expect(m.rows.value).toEqual([])

    let resolveFetch!: (v: unknown) => void
    mocks.collectNetworkInfo.mockReturnValue(new Promise((r) => { resolveFetch = r }))
    const p = m.fetch('i1')
    expect(m.loading.value).toBe(false)
    resolveFetch({ info: { map: { i1: { routes: [] } } } })
    await p
    expect(m.loading.value).toBe(false)
  })
})
```

- [ ] **Step 2: 运行测试确认失败**

```powershell
Set-Location D:\Project\anf-easytier\easytier-gui
pnpm vitest run src/composables/members.test.ts
```

Expected: `useMembers › 已有数据时后台刷新不置 loading` FAIL——当前实现第二次 fetch 也置 `loading = true`。

- [ ] **Step 3: 最小实现**

修改 `members.ts` 的 `fetch`：

```ts
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
```

- [ ] **Step 4: 运行测试确认通过**

```powershell
pnpm vitest run src/composables/members.test.ts
```

Expected: 全部 PASS（含原有 `normalizeMembers` 用例）。

- [ ] **Step 5: 提交**

```powershell
git add easytier-gui/src/composables/members.ts easytier-gui/src/composables/members.test.ts
git commit -m "fix(anf): 成员列表后台刷新不再弹 loading 遮罩（修复闪烁）"
```

### Task 2: 停止连接时删除运行实例（虚拟网卡消失）

**Files:**
- Modify: `easytier-gui/src/composables/anf_first_screen.ts:267-276`（`stop` 函数）+ 顶部 backend import
- Test: `easytier-gui/src/composables/anf_first_screen.test.ts`

**Interfaces:**
- Consumes: `listNetworkInstanceIds()`、`deleteNetworkInstance(instanceId: string)`（backend.ts 已存在）。
- Produces: `stop()` 行为变化——断开 web 客户端后枚举 `running_inst_ids` 并逐个 `deleteNetworkInstance`，失败仅告警，最终 `status = 'idle'`。

- [ ] **Step 1: 写失败测试**

在 `anf_first_screen.test.ts` 的 `mocks` 里加 `deleteNetworkInstance: vi.fn()`，`beforeEach` 加 `mocks.deleteNetworkInstance.mockResolvedValue(undefined)`。

新增用例：

```ts
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
  mocks.listNetworkInstanceIds.mockResolvedValue({ running_inst_ids: ['i1'], disabled_inst_ids: [] })
  mocks.deleteNetworkInstance.mockRejectedValue(new Error('删除失败'))
  const s = useAnfFirstScreen()
  await s.stop()
  expect(s.status.value).toBe('idle')
})
```

- [ ] **Step 2: 运行测试确认失败**

```powershell
pnpm vitest run src/composables/anf_first_screen.test.ts
```

Expected: 第一个新用例 FAIL——`deleteNetworkInstance` 未被调用。

- [ ] **Step 3: 最小实现**

`anf_first_screen.ts` 顶部 import 增加 `deleteNetworkInstance`，`stop()` 改为：

```ts
/** 停止：断开 config-server 连接，并停止/删除由服务器下发的运行实例（虚拟网卡随之消失）。 */
async function stop() {
  cleanup()
  try {
    await initWebClient(undefined, undefined, undefined)
  } catch {
    // 忽略断开错误
  }
  try {
    const { running_inst_ids } = await listNetworkInstanceIds()
    for (const raw of running_inst_ids ?? []) {
      const id = instanceIdToStr(raw)
      if (!id) continue
      try {
        await deleteNetworkInstance(id)
      } catch (e) {
        console.warn('anf stop: remove network instance failed', id, e)
      }
    }
  } catch (e) {
    console.warn('anf stop: list network instances failed', e)
  }
  status.value = 'idle'
}
```

- [ ] **Step 4: 运行测试确认通过**

```powershell
pnpm vitest run src/composables/anf_first_screen.test.ts
```

Expected: 全部 PASS（含原有用例，如 `stop 断开 WebClient 并回到 idle`——默认 mock 无运行实例时不调用 delete）。

- [ ] **Step 5: 提交**

```powershell
git add easytier-gui/src/composables/anf_first_screen.ts easytier-gui/src/composables/anf_first_screen.test.ts
git commit -m "fix(anf): 停止连接时删除运行实例，虚拟网卡真正断开"
```

### Task 3: 全量验证与类型/风格检查

**Files:** 无新增。

- [ ] **Step 1: 跑全量 GUI 单测**

```powershell
pnpm vitest run
```

Expected: 全部 PASS。

- [ ] **Step 2: ESLint 检查**

```powershell
pnpm lint
```

Expected: 无错误（仅对本次改动文件负责）。

- [ ] **Step 3: 类型检查**

```powershell
pnpm exec vue-tsc --noEmit
```

Expected: 无类型错误。

- [ ] **Step 4: 复核 diff 后提交收尾**

```powershell
git status --short
git diff --stat
```

确认无遗漏改动；若 Step 1-3 有连带修改则一并提交。
