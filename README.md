# ANF 平台架构（ANF EasyTier）

> ANFAGENT-30 是基于 [EasyTier](https://github.com/EasyTier/EasyTier) 的中心化组网改造：
> 由中心 `easytier-web` 承担设备授权 / 审批 / 网络与 ACL 管理，设备以稳定 `machine_id` 为授权单元，
> 凭邀请码注册，管理员放行后才入网。网络实例硬隔离 + tag / ACL 图形化、默认拒绝。

[简体中文](/README.md) | [English](/README_CN.md)

> 本文档为 ANF 平台架构的主文档（以中文为主，专业术语用英文辅助）。上游 EasyTier 简介见文末「与上游的关系」。

## 1. 一句话说明

把 EasyTier 从"点对点 / 去中心化组网"改造成**设备需审批的中心化组网**：

- **中心**：`easytier-web`（设备授权 / 审批 / 网络与 ACL 管理）+ 中心 `easytier-core`（中继 / 兜底）。
- **设备**：以机器码（`machine_id`）为唯一授权单元，凭邀请码注册，管理员放行后才下发托管配置入网。
- **网络**：网络实例（network instance）硬隔离，dev TUN 名为 `anf_et`；tag / ACL 默认拒绝，可图形化配置。

## 2. 为什么这样设计 / 与去中心化的区别

原生 EasyTier 是**去中心化**的：节点平等，入网只需提供相同的 `--network-name` 与 `--network-secret`。
这在多人协作 / 企业场景下缺少「谁能进」的管控。ANF 平台架构把「谁能进」交给中心：

| 维度 | 原生 EasyTier | ANF 平台架构 |
| --- | --- | --- |
| 授权单元 | 网络名 + 网络密钥 | 稳定机器码（网卡 + CPU 推导） |
| 入网方式 | 自行填写相同参数 | 邀请码注册 → 管理员审批放行 |
| 配置来源 | 本机 toml / 命令行 | 中心统一下发托管配置 |
| 网络安全 | 依赖 network_secret | 网络实例硬隔离 + tag / ACL 默认拒绝 |
| 权限 | 平等 | 管理员集中管控 |

## 3. 组件与产物

| 组件 | 说明 | 产物 |
| --- | --- | --- |
| `easytier-web` | 中心管理后端（设备审批 / 网络 / ACL / config-server） | `anf-easytier-web` 二进制 + Docker 镜像 |
| `easytier-core` | 中心 / 客户端核心（中继、TUN、加密） | `anf-easytier-core`、`anf-easytier-cli`、`anf-easytier-game` |
| `easytier-gui` | Windows 客户端 GUI（ANF 快速连接） | `anf-easytier.exe` + `wintun.dll` 等 |

### 发布物命名规范

统一使用：`anf_<版本>_<平台>_<架构>.zip`

示例：

```text
anf_2.6.4_windows_x64.zip
anf_2.6.4_macos_arm64.zip
anf_2.6.4_linux_x86_64.zip
```

- 版本号：`2.6.4`（与底层协议 / 核心一致）。
- 平台：`windows` / `macos` / `linux`。
- 架构：`x64` / `arm64` / `x86_64`（按平台习惯）。

> 历史旧命名 `anf-easytier-win-x64-2.6.4-anf.1`、`anf_平台架构_2.6.4_windows_x64` 已废弃，统一改为上表规范。

## 4. Windows 客户端 GUI 功能

### 首页「ANF 快速连接」

- **连接配置（自动保存）**：可新建 / 删除 / 切换多套中心服务器地址，切换会先保存当前项；关闭 GUI 后再打开自动回填上次成功的配置。
- **服务器地址**：填一个可访问的公网 / 局域网 IP + 端口（支持 `tcp` / `udp` / `ws`），例如 `10.0.0.6:22020`。
- **设备昵称**：自定义，展示给同网络成员。
- **机器码**：设备唯一标识，不可修改；管理员以此审核放行。机器码由硬件（网卡 + CPU）推导，同机不变。
- **启动 / 停止**：连接中心 → 等待审批 → 审批放行后建 TUN 入网；按钮状态随审核状态变化（连接中… / 审核中… / 停止 / 重试）。
- **高级**：内联展开信息（网络名称（只读，中心下发）、TUN 网卡名 `anf_et`、配置源、网络密钥提示）。

### 运行环境

- Windows 平台需**以管理员身份运行**才能创建 TUN 虚拟网卡（当检测到非管理员时会给出提示）。
- 创建 TUN **不需要 Npcap**（随包含 `wintun.dll`）；Npcap 仅在子网代理 / KCP 代理 / UDP 广播捕获等场景才需要。

### 模式

- ANF 平台架构客户端**仅保留客户端模式（normal）**，未提供服务器 / 远程等其它模式（这些非本产品特性）。
- 底部下方已移除「切换模式」入口与相关高级配置。

## 5. 中心管理与审批流程

1. 设备在客户端填服务器地址 + 昵称，点「启动」→ 连上中心 config-server，状态为「待审批」。
2. 管理员打开 `easytier-web` 后台（设备审批页），看到全部设备（含待审批 / 已放行 / 已拒绝 / 已踢出）。
3. 管理员点「放行」前，系统会**引导先分配 Tag 与网络实例**（缺失则提示去分配）。
4. 放行后中心为设备生成托管配置（含虚拟 IP、ACL、网络名 / 密钥、中心 peer），通过 config-server 下发。
5. 客户端收到配置后自动建 TUN（`anf_et`），分配虚拟 IP，连上中心 peer，完成入网。
6. 管理员可随时「拒绝 / 踢出 / 编辑 / 删除」，网络与 ACL 变更会热更新到已放行设备。

### 关于网络名称 / 网络密钥是否必填

在 ANF 中心化模式中，**网络名称与网络密钥（`network_secret`）都不是客户端必填项**：

- 二者由中心平台统一管理（服务端参数 `ET_ANF_NETWORK_NAME` / `ET_ANF_NETWORK_SECRET`），客户端只需填「服务器地址 + 设备昵称」。
- 客户端**不持久化网络密钥**：密钥只在运行时内存持有，不出现在本机 `config.toml`，避免明文泄露。
- 详见 [网络名称/密钥通信层调研](docs/anfagent-30/12-anf-network-name-secret-security-2026-08-23.md)。

## 6. 安全模型

- **机器码授权**：`machine_id` 由网卡 + CPU 硬件推导，作为设备的稳定授权单元。
- **中心审批**：设备邀请码注册，管理员放行才下发配置。
- **网络隔离 + ACL 默认拒绝**：多网络实例硬隔离，跨网络默认不通；tag / ACL 规则默认 `drop`，仅显式放行才通。
- **网络密钥**：用于成员身份证明（HMAC-SHA256），非数据加密密钥；数据链路加密由 easytier AES-GCM / WireGuard 负责。
- **控制通道加密**：config-server 连接默认升级为 `Noise_NN_25519_ChaChaPoly_SHA256` 安全隧道（AES-GCM），失败时退回 legacy 明文通道（见调研文档第 5 节风险）。

## 7. 开发与构建

### 环境依赖

- Rust（`rust-toolchain.toml`）、Node.js / pnpm（含 `easytier-frontend-lib`）、protoc、LLVM、7-Zip。
- Windows 需要 VS 开发环境（vcvars64）。

### 构建中心 web

```bash
cargo build -p easytier-web
```

### 构建 Windows 客户端免安装包

```powershell
powershell scripts/build-windows-portable.ps1
```

产物输出到 `dist/`，命名遵循 `anf_2.6.4_windows_x64.zip`。

> 构建需预先设置（详见 `docs/anfagent-30/06-config-distribution-plan.md`）：
> `LIBCLANG_PATH`、`PROTOC`、PATH 加 7-Zip；corepack 目录重定向 `COREPACK_HOME`，避免 ACL 只读导致 pnpm install EPERM。

### 测试

```bash
cargo test -p easytier-web
cargo test -p easytier-core
pnpm --filter easytier-gui test
```

## 8. 与上游 EasyTier 的关系

本项目 fork 自 [EasyTier](https://github.com/EasyTier/EasyTier)，在上游基础上做了 ANFAGENT-30 中心化改造。
上游的通用能力（去中心化组网、共享节点、子网代理、WireGuard 集成等）在 ANF 平台架构中**不作为默认特性暴露**，
但底层协议与核心（加密、NAT 穿透、路由等）保持不变。

如需了解原生 EasyTier 的完整特性与去中心化用法，请见 [README_CN.md](/README_CN.md)（英文辅助）与上游仓库。

## 9. 相关文档

| 文档 | 内容 |
| --- | --- |
| [docs/anfagent-30/00-plan.md](docs/anfagent-30/00-plan.md) | 方案确认稿：背景、设计决策树、架构、里程碑 |
| [docs/anfagent-30/05-config-distribution-design.md](docs/anfagent-30/05-config-distribution-design.md) | 配置自动下发设计 |
| [docs/anfagent-30/11-anf-naming-config-2026-08-22.md](docs/anfagent-30/11-anf-naming-config-2026-08-22.md) | 命名 / 版本 / 配置口径（`2.6.4`、`anf_2.6.4_windows_x64`） |
| [docs/anfagent-30/12-anf-network-name-secret-security-2026-08-23.md](docs/anfagent-30/12-anf-network-name-secret-security-2026-08-23.md) | 网络名称 / 密钥通信层调研 |

## License

本项目许可证与上游 EasyTier 一致，沿用 [LGPL-3.0](https://github.com/EasyTier/EasyTier/blob/main/LICENSE)。
