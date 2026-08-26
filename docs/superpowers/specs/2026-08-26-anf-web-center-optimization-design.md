# ANF Web 中心优化 · 设计树（定稿）

> 状态：定稿（2026-08-26）
> 分支：`codex/anf-web-center-design-tree`（基于 `main`）
> 需求来源：用户问题描述（Web 中心若干优化：端口/连接信息提示、网络随机网段、Tag 交互与编辑、ACL 编辑）+ 设计树第一轮访谈（六问全按推荐）

## 1. 背景与目标

ANFAGENT-30（魔改 EasyTier 成中心化组网）M1–M3 已完成并合入。本轮对中心管理后台（`easytier-web`，REST + config-server + embed 前端）做体验与信息完整性优化，核心目标：

1. **管理员能一眼看到中心各端口/服务对应关系**，并明确「config-server 填什么、转发中心填什么」的实时连接信息，便于给客户端下发正确地址。
2. **网络管理**：新建网络弹窗中网段不填则自动分配随机网段，名称必填。
3. **Tag 管理**：创建交互与网络管理保持一致（弹窗形式），名称必填；创建后可编辑昵称，ID 不可编辑。
4. **ACL 规则**：创建后也可编辑，ID 不可编辑。

## 2. 现状事实（已核对代码）

| 事实 | 现状 |
| --- | --- |
| 端口（仓库配置） | `deploy/compose.anf.yaml`：web `11211`、config-server UDP `22020`、中心 core `11110`（避开官方 core 的 11010）；`deploy/compose.yaml`：`11211 / 22020 / 11010` |
| 端口（用户提供生产值） | 15211 / 25220 / 13110（与仓库默认不同，以运行时部署配置为准，见 Q7/Q11） |
| 网络创建 | `POST /api/v1/networks`：`cidr` 可选；为空则存 `NULL`，配置生成与虚拟 IP 分配回退到固定 `DEFAULT_NETWORK_CIDR = 10.144.0.0/24`（`easytier-web/src/anf/config.rs`）→ 多网络共享同一默认网段 |
| 网络名称校验 | `create_network` 仅 `trim`，空名称可入库；前端 `InputText` 未强制必填 |
| Tag 管理 | 仅 `POST /api/v1/tags` / `GET` / `DELETE /tags/:id`，**无更新端点**；前端为「顶部输入框 + 创建按钮」的简易交互，与网络管理的弹窗形式不一致 |
| Tag 引用方式 | `device_tags` 以 **tag 名字符串** 存储；ACL 规则 `source_tags/destination_tags` 也是 tag 名字符串（JSON）→ 改名需级联，否则设备/规则引用悬空 |
| ACL 规则 | 后端已有 `PATCH /api/v1/networks/:id/rules/:ruleId`（`update_acl_rule` + 变更后 `reconcile_after_acl_change` 热更新）；**前端无编辑 UI** |
| Dashboard | 仅显示 `device_count`（上游语义：当前登录用户绑定的 machine 数），且**每 1 秒轮询** |
| 前端遗留 | 登录页带注册/验证码/API Host 切换等上游交互；多处硬编码英文（`Login Failed`、`Device Count`）；Logo 仍为 `easytier.png`；密码仍为 MD5（`ts-md5`）；预置 `admin/admin` 与 `user` 账号 |
| 测试载体 | `cargo test -p easytier-web`（现有 110+ 用例）、`pnpm --dir easytier-web/frontend build`（vue-tsc + vite）、VM 端到端冒烟 |

## 3. 设计决策汇总（设计树）

### 3.1 第一轮访谈（已确认：全按推荐）

| # | 决策 | 结论 |
| --- | --- | --- |
| Q1 | 优化范围 | A（安全加固）+ B（后台 UI/UX 品牌化）本轮；C 只做低风险项（轮询降频、列表筛选）；D 视情况 |
| Q2 | 安全加固 | 2.1 密码哈希 MD5→argon2、2.2 默认 `admin` 强制首登改密 + 禁用预置 `user`、2.3 登录限流、2.4 会话过期/失效 —— 本轮；2.5 审计只做最小版（表 + API，不做页面） |
| Q3 | UI 品牌化 | 复用 GUI 已定的 PrimeVue `definePreset` 品牌体系（indigo `#6366f1` → violet `#8b5cf6`），Web 端轻量落地；登录页品牌化、侧边栏 ANF Logo、文案补 i18n |
| Q4 | Dashboard/列表 | Dashboard 统计扩充 + 轮询 1s→10s + 设备列表状态 Tab/关键字筛选本轮；批量操作二期 |
| Q5 | 协议层 | 本轮一律不碰 config-server/客户端协议（明文回退收敛、注册流程等另立一项）；注册页残留只做前端隐藏/下架 |
| Q6 | 验收 | `cargo test -p easytier-web` 全绿 + `vue-tsc`/vite 构建通过 + VM 端到端冒烟（登录 → 审批 → ACL 变更 → 页面确认） |

### 3.2 本轮四项核心（用户问题驱动）

| # | 决策 | 结论 |
| --- | --- | --- |
| Q7 | 端口/连接信息展示 | **数据驱动**：新增 `GET /api/v1/center/info`（管理员），返回运行时实际端口与中心参数（api 端口、config-server 端口/协议、网络名、中心 peer URL、版本）；前端 Dashboard 新增「中心连接信息」卡片 + 端口→服务表，不做硬编码。host 取自浏览器当前访问地址（管理员经 mesh 访问 web，与 config-server/中心同主机） |
| Q8 | 网络随机网段 | 后端 `create_network`：`cidr` 为空时自动生成随机 `10.x.y.0/24`（排除已存在网段与保留段 `10.126.0.0/16` mesh、`10.144.0.0/24` 示例默认段，重试上限 16 次）；替代固定 `DEFAULT_NETWORK_CIDR` 回退；`cidr` 若填写则校验合法 IPv4 CIDR（/8–/30）；`name` 必填（前后端双重校验） |
| Q9 | Tag 编辑 | 新增 `PATCH /api/v1/tags/:id`（body 仅 `name`）；后端校验 `valid_tag_name` + 唯一性；改名**级联**更新 `device_tags.tag` 与所有 ACL 规则的 `source_tags/destination_tags` JSON 引用，并触发受影响网络 reconcile（热更新）；ID 只读 |
| Q10 | ACL 编辑 | 复用已有 `PATCH /api/v1/networks/:id/rules/:ruleId`（已含 reconcile，后端零改动）；前端补「编辑」入口 + 预填弹窗，ID 只读 |
| Q11 | 端口差异处理 | 展示一律以运行时配置为准（Q7）；文档标注仓库默认值（11211/22020/11110 或 11010）与用户提供的生产期望值（15211/25220/13110），部署实际值自动正确显示 |

### 3.3 明确不做 / 二期

- 批量放行/拒绝/踢出（Q4.4）。
- SSE/WebSocket 推送（本期只降轮询频率）。
- config-server 协议层改动：legacy 明文回退收敛、设备注册流程改造（Q5）。
- 审计日志管理页面（本期仅落表 + API）。
- 网络实例网段创建后修改、扩容（本期不支持修改 cidr）。

## 4. 详细设计

### 4.1 中心连接信息（端口 → 服务映射 + 客户端填写提示）

**后端**

- 在 `FeatureFlags`（或新增 `CenterInfo` 结构放入 `AppState`）补充：`api_server_port`、`config_server_protocol`、`config_server_port`、`web_server_port`（可选）；`anf_network_name` / `anf_center_peer_url` 已有。
- 新增管理员接口 `GET /api/v1/center/info`，返回：

```json
{
  "version": "2.6.4-anf.1",
  "api_server_port": 11211,
  "web_server_port": null,
  "config_server_protocol": "udp",
  "config_server_port": 22020,
  "anf_network_name": "anf-m3",
  "anf_center_peer_url": "tcp://10.126.126.6:11110"
}
```

**前端（Dashboard 新增「中心连接信息」卡片）**

| 端口 | 协议 | 服务 | 作用 | 客户端填写 |
| --- | --- | --- | --- | --- |
| `api_server_port`（11211） | TCP | Web 控制台 / REST API | 管理员后台 | — |
| `config_server_port`（22020） | UDP | config-server | 设备注册 / 配置下发 | 服务器地址填 `<host>:<port>` |
| 中心 core（`anf_center_peer_url` 解析，11110/11010） | TCP+UDP | 中心中继 / 兜底 | 转发中心 peer | 转发中心填 `anf_center_peer_url`（如 `tcp://<host>:11110`） |

- 每行提供「复制」按钮；`<host>` 由前端从当前访问地址 `location.hostname` 推导（管理员经 ANF mesh 访问 web，与 config-server / 中心 core 同主机）。
- 表尾说明：端口以本实例运行时配置为准；生产若使用 15211 / 25220 / 13110，本表将如实显示实际值。
- 附网络名 `anf_network_name` 与 `version` 展示（可复制）。

### 4.2 网络管理：随机网段 + 名称必填

**后端（`easytier-web/src/db/anf_networks.rs` + `restful/networks.rs`）**

- `create_network` 增加校验与生成逻辑：
  - `name` trim 后非空，否则 `InvalidInput("网络名称不能为空")`；
  - `cidr` 为空/空白 → 随机生成 `10.{a}.{b}.0/24`（`a∈[1,254]`、`b∈[0,255]`），与库内既有 `cidr`、保留段 `10.126.0.0/16`、`10.144.0.0/24` 冲突则重试，上限 16 次，失败返回 422；
  - `cidr` 已填写 → 校验为合法 IPv4 CIDR（前缀 /8–/30），非法返回 422；
  - 生成的网段写入 `network_instances.cidr`，配置生成与虚拟 IP 分配不再回退共享默认段。
- 新增单元测试：随机网段格式/唯一性/保留段排除、空名称拒绝、非法 cidr 拒绝、显式 cidr 原样保留。

**前端（`NetworkManagementPage.vue`）**

- 弹窗中「名称」必填（`required` + 提交前 trim 校验，空则 Toast 提示）；
- 「网段」占位与说明改为「可选，留空自动分配随机网段（如 10.x.y.0/24）」；
- 创建成功后 Toast 显示实际网段（自动分配值或用户填写的值）。

### 4.3 Tag 管理：交互与网络管理一致 + 可编辑昵称

**后端（`restful/tags.rs` + `db/anf_networks.rs`）**

- 新增 `PATCH /api/v1/tags/:id`（管理员），body `{ "name": string }`：
  - `valid_tag_name` 校验（字母/数字/中划线/下划线/点，≤32 字符，不含空白）；
  - 与既有 tag 重名 → 422；
  - 级联更新：`device_tags.tag` 旧名 → 新名；所有 ACL 规则 `source_tags` / `destination_tags` JSON 数组中旧名 → 新名；
  - 对引用该 tag 的网络触发 `reconcile`（与 ACL 变更同机制，热更新到已放行设备）。
- 新增单元测试：改名成功、重名校验、级联 device_tags、级联 ACL 规则引用、reconcile 触发。

**前端（`TagManagementPage.vue`）**

- 「新建 Tag」改为与网络管理一致的 **按钮 + Dialog** 形式：Dialog 内「名称」必填（placeholder 沿用「字母/数字/中划线/下划线/点」规则）；
- 行操作新增「编辑」：打开同一 Dialog 的编辑模式，顶部只读展示 ID，名称可编辑；保存调 `PATCH /api/v1/tags/:id`；
- 「删除」保留；刷新/Toast 行为与网络管理一致。

### 4.4 ACL 规则编辑

**后端**：已具备 `PATCH /api/v1/networks/:id/rules/:ruleId`（含 `update_acl_rule` 与 `reconcile_after_acl_change`），本期**零改动**；补 1–2 个前端联调用例即可。

**前端（`AclEditorPage.vue` + `modules/api.ts`）**

- `api.ts` 新增 `updateAclRule(networkId, ruleId, rule)`；
- 规则表行操作新增「编辑」：打开现有创建 Dialog 的编辑模式，全部字段预填（名称 / 启用 / 源 tag / 目标 tag / 协议 / 端口 / 动作 / 优先级），**ID 只读**展示；保存调 PATCH，成功后刷新列表（热更新由后端 reconcile 完成，无需额外动作）。

### 4.5 附带项（已接受推荐，拆任务实施）

- **Dashboard 扩充**：新增统计卡（设备总数 + 按状态分布 pending/approved/rejected/kicked、网络数、tag 数、ACL 规则数、在线会话数）；轮询 1s → 10s。ANF 统计走新接口或扩展 `/api/v1/summary`（上游 `device_count` 语义保留）。
- **设备列表**：状态 Tab 筛选 + 关键字搜索（低风险版，不分页后端改造）。
- **品牌化轻量落地**：复用 GUI 的 PrimeVue preset 主色（indigo→violet）；登录页品牌化；侧边栏 Logo 换 ANF；新增文案一律走 i18n（中文默认）。
- **安全加固工作包（独立任务拆分，不阻塞四项核心）**：密码 MD5→argon2（前后端登录协议同步改）、默认 `admin` 强制首登改密、禁用预置 `user`、登录限流、会话过期；审计最小版（表 + API）。
- **清理**：登录页注册/API Host 切换等上游残留入口（仅前端隐藏/下架）；移除 `console.log` 调试输出。

## 5. 验收标准

1. `cargo test -p easytier-web` 全绿（新增用例覆盖：随机网段、tag 改名级联、center/info、规则编辑、名称必填）。
2. `pnpm --dir easytier-web/frontend build`（vue-tsc + vite）通过。
3. 四项核心功能手动验证：
   - 中心信息卡片显示端口/服务/填写提示，复制按钮可用；
   - 新建网络不填网段 → 自动获得随机网段；名称必填；
   - Tag 新建弹窗与网络管理一致；改名后设备引用与 ACL 规则同步、设备侧热更新；
   - ACL 规则编辑生效（改动作/优先级后热更新）。
4. VM 端到端冒烟：登录 → 中心信息 → 建网络（随机网段）→ 建 tag → 改名 → 建规则 → 编辑规则 → 设备审批/放行链路正常。
5. 页面截图确认品牌化与中文文案。

## 6. 影响面（文件清单）

**后端（Rust）**

- `easytier-web/src/main.rs`：FeatureFlags / CenterInfo 补充端口与协议字段；
- `easytier-web/src/restful/mod.rs` 或新增 `restful/center.rs`：`GET /api/v1/center/info` + summary 扩展；
- `easytier-web/src/restful/networks.rs`：名称/cidr 校验接线；
- `easytier-web/src/db/anf_networks.rs`：`create_network` 随机网段、`update_tag` + 级联、`valid_tag_name`/唯一性；
- `easytier-web/src/anf/config.rs`：随机网段生成器（或独立模块），移除对 `DEFAULT_NETWORK_CIDR` 的隐式回退依赖；
- `easytier-web/src/restful/tags.rs`：PATCH 端点；
- 安全工作包：`restful/auth.rs` / `restful/users.rs`（argon2、限流）、`migrator`（密码哈希迁移/审计表，独立任务）。

**前端（Vue）**

- `easytier-web/frontend/src/modules/api.ts`：`centerInfo` / `updateTag` / `updateAclRule` / summary 扩展；
- `easytier-web/frontend/src/components/Dashboard.vue`：连接信息卡 + 统计卡 + 轮询 10s；
- `easytier-web/frontend/src/components/NetworkManagementPage.vue`：必填 + 随机网段提示；
- `easytier-web/frontend/src/components/TagManagementPage.vue`：Dialog 化 + 编辑；
- `easytier-web/frontend/src/components/AclEditorPage.vue`：编辑弹窗；
- `easytier-web/frontend/src/components/DeviceAdminPage.vue` / `DeviceList.vue`：筛选（低风险）；
- `easytier-web/frontend/src/components/Login.vue` / `MainPage.vue` / `main.ts` / `style.css`：品牌化 + 去残留；
- `easytier-web/frontend-lib/src/locales/cn.yaml` / `en.yaml`：新增文案。

## 7. 风险与待确认

- **端口差异**：仓库默认 11211 / 22020 / 11110（或 11010），用户提供生产值为 15211 / 25220 / 13110。设计采用数据驱动，展示值随运行时配置自动正确；如需在文档/截图中固化生产值，请以实际部署 env 为准确认。
- **Tag 改名级联**：本期实现 `device_tags` + ACL 规则 JSON 引用级联与网络 reconcile；存量数据迁移无需手工干预。
- **MD5→argon2**：涉及登录协议变更，独立任务，与四项核心互不阻塞；迁移期需支持旧哈希平滑过渡或强制改密。
- **随机网段**：/24 满足单网络 ≤254 台设备；创建后不支持修改网段（二期）。
- **Dashboard 统计口径**：上游 `device_count` 是「当前用户机器数」，ANF 统计将新增独立口径（设备表），不破坏上游语义。
