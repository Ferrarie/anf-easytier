# ANF 平台架构（ANF EasyTier）

> **本文档是仓库唯一的功能需求与权威说明（single source of truth）。**
> 基于 [EasyTier](https://github.com/EasyTier/EasyTier) 的中心化组网改造。
> 历史规划 / 设计文档已并入本文，不再单独维护；git 历史保留全部内容。

## 1. 产品定位

把 EasyTier 从"点对点 / 去中心化组网"改造成**设备需审批的中心化组网**：

- **中心**：`easytier-web`（设备授权 / 审批 / 网络与 ACL 管理 + config-server）+ 中心 `easytier-core`（中继 / 兜底）。
- **设备**：以机器码（`machine_id`）为唯一授权单元，凭邀请码注册，管理员放行后才下发托管配置入网。
- **网络**：网络实例（network instance）硬隔离，TUN 网卡名为 `anf_et`；tag / ACL 默认拒绝，可图形化配置。

## 2. 与上游 EasyTier 的关系

本项目 fork 自 [EasyTier](https://github.com/EasyTier/EasyTier)（基线 `v2.6.4-66-g57eb6908`）。
底层协议与核心（加密、NAT 穿透、路由、中继）保持兼容；上游的"自行填写网络名 + 密钥即可入网"的
去中心化模式在 ANF 中**不作为默认特性暴露**，入网必须经过中心审批。

## 3. 核心概念与术语

| 术语 | 含义 |
| --- | --- |
| `machine_id` | 设备唯一机器码，由硬件（网卡 + CPU）推导，同机不变，是授权单元 |
| 邀请码（invite code） | 设备注册凭证，由管理员生成，可限次数 / 过期时间 |
| 审批 / 放行 | 管理员将 pending 设备置为 approved，才下发托管配置 |
| 网络实例（network instance） | 一组设备的隔离虚拟网络，分配独立网段（cidr）与虚拟 IP |
| tag | 设备分组标签，ACL 规则按 tag 匹配 |
| ACL | 网络内 tag 之间的访问控制规则，默认 `drop` |
| config-server | 中心 UDP 服务（默认 22020），负责设备注册与配置下发 |
| 托管配置 | 中心生成的完整连接配置（网络名 / 密钥、虚拟 IP、ACL、中心 peer 地址） |

## 4. 总体架构

```text
┌─────────────────────────── 中心（VM / 服务器）───────────────────────────┐
│  easytier-web（anf-easytier-web）                                        │
│    ├─ REST API（/api/v1/*，管理后台 + 客户端接口）  :11211（仅 mesh 内网） │
│    ├─ config-server（设备注册 / 配置下发，UDP）    :22020                │
│    └─ SQLite（/app/data/et.db：设备/邀请码/网络/tag/ACL）                 │
│  easytier-core（中心中继 / 兜底）                :11010 或 :11110        │
└──────────────────────────────────────────────────────────────────────────┘
         │ 邀请码注册 + 审批后配置下发（config-server / Noise 隧道）
         ▼
┌─────────────────────────── 客户端（Windows）────────────────────────────┐
│  anf-easytier.exe（Tauri GUI：ANF 快速连接 / 成员列表 / 房间信息）        │
│  anf-easytier-core（TUN 网卡 anf_et / 加密 / 路由）                      │
└──────────────────────────────────────────────────────────────────────────┘
```

### 端口约定

| 端口 | 协议 | 用途 | 暴露范围 |
| --- | --- | --- | --- |
| 11211 | TCP | web 控制台 / REST API | 仅 mesh 内网 |
| 22020 | UDP | config-server（注册 / 下发） | 公网 |
| 11010 | TCP+UDP | peer / 中继 | 公网 |

## 5. 功能需求

### 5.1 设备接入流程（注册 → 审批 → 放行 → 入网）

1. 客户端填写服务器地址（`ip:port`，支持 `tcp` / `udp` / `ws`，如 `10.0.0.6:22020`）+ 设备昵称，点击「启动」。
2. 客户端连上中心 config-server，凭邀请码注册，状态为 `pending`（待审批）。
3. 管理员在后台设备审批页看到待审批设备；放行前系统**引导先分配 Tag 与网络实例**（缺失时给出提示）。
4. 放行后中心为设备生成托管配置（虚拟 IP、ACL 编译结果、网络名 / 密钥、中心 peer），通过 config-server 下发。
5. 客户端收到配置后自动创建 TUN（`anf_et`）、分配虚拟 IP、连上中心 peer，完成入网。
6. 管理员可随时「拒绝 / 踢出 / 编辑 / 删除」；已放行设备的网络或 ACL 变更会**热更新**并重新下发。

设备状态机：`pending → approved / rejected / kicked`；`approved → rejected / kicked`。

### 5.2 网络实例

- 网络实例硬隔离，各自独立网段（cidr）与虚拟 IP 池；设备只能进入被分配的网络。
- 删除网络时，仅"已放行"设备计入占用；pending / rejected 设备的历史引用会被自动清理，避免"成员数为 0 却删不掉"。

### 5.3 tag 与 ACL

- tag 是设备分组标签，可多选；网络内的 ACL 规则以"源 tag → 目标 tag"表达。
- ACL 默认 `drop`：未显式放行的跨 tag 流量一律拒绝；规则变更会热更新到该网络全部已放行设备。

### 5.4 配置下发（config-server）

- 中心统一管理网络名与网络密钥（服务端参数），客户端**不需要也不能持久化网络密钥**（仅运行时内存持有）。
- 客户端只需"服务器地址 + 设备昵称"；`network_name` / `network_secret` 由中心下发，不出现在本机 `config.toml`。
- config-server 连接默认升级为 `Noise_NN_25519_ChaChaPoly_SHA256`（AES-GCM）安全隧道；协商失败时退回 legacy 明文通道（已知风险点，见安全模型）。

### 5.5 中心管理后台（REST API）

公开接口：

- `POST /api/v1/devices/register`：设备凭邀请码注册
- `/api/v1/auth/login` / `logout` / `captcha` / `register`（注册可关闭）
- `/api/v1/machines*`：客户端机器 / 网络实例管理（启动 / 停止 / 配置）

管理接口（需管理员会话）：

- `GET /api/v1/devices?status=`：设备列表；`POST /devices/:id/approve|reject|kick`；`PATCH|DELETE /devices/:id`
- `POST|GET /api/v1/invites`、`DELETE /invites/:id`：邀请码管理
- `POST|GET /api/v1/networks`、`DELETE /networks/:id`、`GET /networks/:id/devices`：网络管理
- `POST|GET /api/v1/networks/:id/rules`、`DELETE /networks/:id/rules/:ruleId`：ACL
- `POST|GET /api/v1/tags`、`DELETE /tags/:id`：tag 管理
- `GET /api/v1/summary`、`GET /api/v1/sessions`、`PUT /api/v1/auth/password` 等

前端页面：登录 / 注册 / 设备注册、Dashboard、设备列表（设备管理）、设备审批、邀请码、网络、tag、ACL 编辑器。

### 5.6 客户端 GUI（anf-easytier.exe）

- **ANF 快速连接（首页）**：服务器地址多套配置自动保存 / 切换；设备昵称自定义；机器码只读展示；「启动 / 停止」按钮状态随审核状态变化（连接中… / 审核中… / 停止 / 重试）；高级区展示只读网络名、TUN 网卡名 `anf_et`、配置源。
- **成员列表**：展示同网络成员（复用 `collect_network_info`）。
- **房间信息窗口**：连接信息展示（IP / 网段等）。
- **运行要求**：创建 TUN 需 Windows 管理员权限（随包含 `wintun.dll`，无需 Npcap）；Npcap 仅在子网代理 / KCP 代理 / UDP 广播捕获场景需要。
- **模式**：仅保留客户端模式（normal），不提供服务器 / 远程等其它模式。

## 6. 安全模型

- **机器码授权**：`machine_id` 由网卡 + CPU 硬件推导，作为设备稳定授权单元。
- **中心审批**：设备邀请码注册，管理员放行才下发配置；未放行设备无任何网络配置。
- **网络隔离 + ACL 默认拒绝**：多网络实例硬隔离；跨网络默认不通；tag / ACL 规则默认 `drop`。
- **网络密钥**：用于成员身份证明（HMAC-SHA256），非数据加密密钥；数据链路加密由 easytier AES-GCM / WireGuard 负责。
- **控制通道加密**：config-server 连接默认 Noise 安全隧道（AES-GCM）；协商失败退回 legacy 明文通道为已知风险。

## 7. 部署

### 7.1 生产部署（docker compose）

```bash
cd deploy
cp .env.example .env          # 按需修改 ANF_NETWORK_NAME / ANF_NETWORK_SECRET / ANF_CENTER_PEER_URL
docker compose -f compose.anf.yaml up -d
```

部署文件说明（`deploy/`）：

- `compose.anf.yaml`：`anf-easytier-web`（11211 + 22020/udp，卷 `web-data:/app/data`）+ `anf-easytier-core`（host 网络，中继 11110，避开官方 core 的 11010）。
- `Dockerfile.web`：将 `deploy/bin/easytier-web`（embed 前端二进制）打进 `ubuntu:24.04` 镜像。
- 环境变量：`ANF_NETWORK_NAME`（默认 `anf-m3`）、`ANF_NETWORK_SECRET`、`ANF_CENTER_PEER_URL`（默认 `tcp://10.0.0.6:11110`）。
- VM 连接凭据存放于仓库根 `.env`（gitignore，不入库）。

### 7.2 更新 web 二进制（embed 前端）

```bash
export PATH=$HOME/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
cd ~/anf-easytier
cargo build --release -p easytier-web --features easytier-web/embed
cp target/release/easytier-web deploy/bin/easytier-web
cd deploy && docker compose -f compose.anf.yaml build easytier-web && docker compose -f compose.anf.yaml up -d easytier-web
```

> 前端源码改动需先在 `easytier-web/frontend` 执行 `pnpm build`（`vue-tsc -b && vite build`）生成 `dist/`，再走上面的 embed 构建。

### 7.3 ⚠️ VM 安全红线

VM 上官方 `easytier@default.service`（anidev 网络）是 VM 在 mesh 里的存在本身：

- **禁止** `systemctl stop/restart easytier@default`、`reboot`、关闭 `tun0`、修改其网络配置；
- 停掉即失去 mesh 通道，需物理机恢复。

Docker 等服务启停不受此限制。

## 8. 开发与构建

### 环境依赖

- Rust（`rust-toolchain.toml` 锁定）、Node.js / pnpm（含 `easytier-frontend-lib`）、protoc、LLVM、7-Zip。
- Windows 需 VS 开发环境（vcvars64）。

### 构建

```bash
# 中心 web（embed 前端）
cargo build --release -p easytier-web --features easytier-web/embed

# 前端（生成 easytier-web/frontend/dist）
cd easytier-web/frontend && pnpm build

# Windows 客户端免安装包
powershell scripts/build-windows-portable.ps1
```

发布物命名：`anf_<版本>_<平台>_<架构>.zip`，例如 `anf_2.6.4_windows_x64.zip`。

### 测试

```bash
cargo test -p easytier-web
cargo test -p easytier-core
cd easytier-gui && pnpm vitest run        # 客户端组合式单测（anf_first_screen / ip_cidr / room_window / members / mobile_vpn）
```

## 9. 仓库目录结构

| 目录 / 文件 | 说明 |
| --- | --- |
| `easytier` / `easytier-core` / `easytier-proto` | 核心库、core 二进制、protobuf 定义（上游兼容） |
| `easytier-web` | 中心管理后端（REST + config-server + embed 前端 + SQLite） |
| `easytier-gui` | Windows 客户端 GUI（Tauri） |
| `easytier-contrib` | 上游扩展（android-jni / ffi / magisk / mini / ohrs / uptime 等） |
| `tauri-plugin-vpnservice` | Tauri VPN 服务插件 |
| `deploy` | 生产部署（compose / Dockerfile / bin） |
| `scripts` | VM 运维（vm_ssh / vm_sftp / 同步 tarball）与打包脚本 |
| `assets` | 文档 / 图标资源 |

## 10. 当前状态

ANFAGENT-30 已完成并合入：

- M1：设备邀请码注册 / 审批 / 放行 / 拒绝 / 踢出，管理员会话；
- M2：网络实例 / tag / ACL 图形化管理，默认拒绝 + 配置热更新；
- M3：中心部署（docker compose）+ config-server 配置下发；
- 客户端：ANF 快速连接首屏、成员列表、房间信息窗口、进程 / 产物改名 ANF、`config.toml` 持久化与 `machine_id`。

## License

与上游 EasyTier 一致，[LGPL-3.0](https://github.com/EasyTier/EasyTier/blob/main/LICENSE)。
