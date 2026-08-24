# GUI 停止连接不断连 / 成员窗口刷新闪烁 修复设计

日期：2026-08-24
范围：ANF EasyTier Windows GUI（`easytier-gui`，Vue 3 + Tauri 2）

## 1. 问题

### 1.1 点击“停止”后并未真正断开

用户点击“停止”后，服务器 web 的成员列表仍能看到该设备在线。

根因：`useAnfFirstScreen.stop()` 只调用 `initWebClient(undefined, undefined, undefined)`，
即仅丢弃 config-server 的 web 客户端（心跳停止），但已运行的网络实例
（TUN 虚拟网卡 + 对等连接）仍留在本地 `INSTANCE_MANAGER` 中，设备继续停留在虚拟网络内，
因此服务器 web 仍显示连接状态、虚拟网卡也未消失。

### 1.2 成员列表窗口刷新时闪烁

根因：`useMembers.fetch()` 每次轮询开始都置 `loading = true`，结束时置 `false`；
成员窗口每 1 秒轮询一次，PrimeVue `DataTable :loading` 每次都会弹出半透明遮罩，
造成肉眼可见的闪烁。

## 2. 设计决策（用户已确认）

1. **“停止”= 完全断开**：断开 config-server 连接，并停止/删除所有本地运行中的网络实例，
   虚拟网卡随之消失。
2. **保留自动重连语义**：`last_instance_id` 不清除，下次启动 GUI 仍自动连回上次服务器。
3. **闪烁修复**：仅首次加载（列表为空）时显示 loading 遮罩；后台 1 秒轮询静默更新。
4. **实现位置**：前端 composable（`anf_first_screen.ts` / `members.ts`），不动 Rust 侧
   `init_web_client(None)` 的全局语义，避免影响 normal 模式其它调用路径。

## 3. 改动方案

### 3.1 `easytier-gui/src/composables/anf_first_screen.ts` — `stop()`

在现有断开 web 客户端之后追加：

1. 调用 `listNetworkInstanceIds()` 枚举运行实例；
2. 对每个实例 ID（经 `instanceIdToStr` 归一化）调用 `deleteNetworkInstance(id)`，
   从实例管理器移除并停止（虚拟网卡消失，本地存储的该实例配置一并清理）；
3. 任何一步失败不阻断：先尝试断开 web 客户端，再尝试删除实例，删除失败仅告警；
4. `status` 置为 `idle`，`lastInstanceId` 保留（自动重连语义不变）。

### 3.2 `easytier-gui/src/composables/members.ts` — `fetch()`

调整 loading 语义：

- 仅当 `rows.value.length === 0` 时置 `loading = true`（首屏）；
- 已有数据时后台刷新不再弹遮罩；
- `finally` 中仍置 `loading = false`，首屏结束遮罩正常消失。

轮询周期保持 1 秒不变。

## 4. 影响面

- `switchProfile()` 依赖 `stop()`：切换配置时同样会停掉旧实例，符合“切换即断开旧中心”的既有注释语义。
- 成员窗口在实例被删除后继续轮询会失败：`collectNetworkInfo` 报错展示在窗口内，属可接受表现，不做额外处理。
- ANF 模式下所有本地实例均由 config-server 下发创建（本 fork 仅有 ANF 首屏 UI），删除全部运行实例安全。

## 5. 测试

- `anf_first_screen.test.ts`：更新 `stop` 用例——断言断开 web 客户端后还会枚举并删除运行实例，
  且异常时仍回 `idle`。
- `members.test.ts`：新增用例——首次 fetch 置 `loading=true`；已有数据时后台 fetch 不置 `loading=true`。

## 6. 非目标

- 不修改 Rust 侧 `init_web_client(None)` 行为。
- 不改变成员窗口轮询周期。
- 不引入“停止后保持断开”的持久化开关。
