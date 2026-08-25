# ANF EasyTier CI/CD 打包指引与失败复盘

> 目标：用 GitHub Actions 稳定产出四类交付物 —— **Android APK、macOS GUI、Windows GUI、Linux 服务端**。
> 本文档沉淀了本仓库从 Gitea Actions 迁移到 GitHub Actions 过程中遇到的全部失败案例、根因与修复方式，并给出提升 CI/CD 成功率的可执行清单。

## 1. 交付物与对应工作流

| 交付物 | 工作流 | 产物 |
| --- | --- | --- |
| Android APK（4 ABI） | `.github/workflows/mobile.yml` | `anf-easytier-mobile-android-{aarch64,armv7,i686,x86_64}` |
| macOS GUI | `.github/workflows/gui.yml` | `anf-easytier-gui-macos-{x86_64,aarch64}`（DMG） |
| Windows GUI | `.github/workflows/gui.yml` | `anf-easytier-gui-windows-{x86_64,i686,arm64}`（NSIS exe） |
| Linux 服务端 | `.github/workflows/core.yml` | `anf-easytier-core/cli` + `easytier-web-embed`（x86_64/aarch64/armv7hf/armhf） |
| 汇总发版 | `.github/workflows/release.yml` | 手动填写各 run id 后打 tag 发 draft release |

触发方式：push 到 `main/develop/releases/**`、PR、以及各工作流的 `workflow_dispatch`（手动）。

## 2. 失败根因复盘（按阶段）

### 2.1 工具链版本错位

| 症状 | 根因 | 修复 |
| --- | --- | --- |
| `cargo fmt` 报 `'cargo-fmt' is not installed for the toolchain '1.95-...'` | `dtolnay/rust-toolchain@stable` 未指定 `toolchain`，组件装到了 stable，而仓库 `rust-toolchain.toml` 强制 1.95 | 显式 `toolchain: 1.95` + `components: rustfmt, clippy` |
| （修复后）`cargo fmt --all --check` 报上游代码格式 diff | `easytier-core` 部分文件与 rustfmt 1.95 规则不一致（上游历史代码） | 需要 `cargo fmt --all` 后提交，或调整 test.yml 的格式 gate |

结论：**工具链必须与 `rust-toolchain.toml` 完全一致**，并在工作流里显式声明。

### 2.2 前端 / pnpm 依赖层

| 症状 | 根因 | 修复 |
| --- | --- | --- |
| `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH` | lockfile 的 `overrides` 与配置不一致：pnpm 9 读 `package.json` 的 `pnpm.overrides`（含 `happy-dom`），lockfile 里只有 `minimatch`；pnpm 11 又不再读 `pnpm.overrides` | 把 overrides 统一移到 `pnpm-workspace.yaml`，并用 **CI 同款 pnpm 版本** 重新生成 lockfile |
| 同样的报错换到 GitHub 后复现 | GitHub 的 prepare-pnpm 用 pnpm **10.34.5**，而 lockfile 是用 pnpm 9.12.1 生成的，pnpm 10 要求 lockfile 里记录 overrides | 用 `pnpm@10.34.5 install --no-frozen-lockfile --lockfile-only` 重新生成并提交 |
| `vue-tsc` 报 `Cannot find module 'tauri-plugin-vpnservice-api'` | 该包 `types/main` 指向 `dist-js/`（rollup 产物），未构建、未提交 | 在 `tauri android build` 前先 `pnpm --dir tauri-plugin-vpnservice build`（prepare-pnpm 全量 `pnpm -r build` 已覆盖） |

结论：**lockfile 必须用与 CI 一致的 pnpm 大版本生成；overrides 只放 `pnpm-workspace.yaml`；workspace 内 JS 包的构建产物要先于 typecheck 产出**。

### 2.3 Android 工程层

| 症状 | 根因 | 修复 |
| --- | --- | --- |
| `cmdline-tools` 下载 404 | `cmdline-tools-version: 12.0` 对应的 URL `commandlinetools-linux-12.0_latest.zip` 不存在 | 改用实际构建号 `12266719`（`commandlinetools-linux-12266719_latest.zip` 有效） |
| `tauri android build` 报 `Project directory .../java/com/anidev/anfeasytier does not exist` | `gen/android` 是上游生成的旧包名 `com.kkrainbow.easytier`，而 `tauri.conf.json` identifier 已改为 `com.anidev.anfeasytier` | 迁移 `gen/android` 包名（目录、`package`、`namespace`、`applicationId`）并同步前端 `disallowedApplications` |
| 前端 codegen 报 `google/protobuf/timestamp.proto: File not found` | Maven Central 的 protoc 是裸二进制，不带 well-known types include | 从 `protobuf-java` jar 解压 `google/protobuf/*.proto` 到 `/usr/local/include`（protoc 按 `../include` 隐式查找） |
| `machine-uid` 报 `unresolved import machine_id` | `machine-uid` 0.5.x/0.6.x 都没有 `target_os="android"` 的实现模块 | 升级 0.6.0 后仍不够：把依赖按 `cfg(not(target_os = "android"))` 门控，代码里 Android 分支返回 `None` 回退 |

结论：**改 Tauri identifier 后必须重新 `tauri android init` 并把 `gen/android` 提交进仓库；新增 Rust 依赖要确认 Android target 可编译**。

### 2.4 Runner 环境层（自托管 / Gitea runner 阶段）

| 症状 | 根因 | 修复 |
| --- | --- | --- |
| setup-protoc 报 `Bad credentials` | 把 Gitea 的 `GITHUB_TOKEN` 传给面向 GitHub API 的 action | 去掉 `repo-token`（GitHub 上则正常传 `GITHUB_TOKEN`） |
| GitHub release 附件下载 TLS 中断 | 自托管 runner 到 `objects.githubusercontent.com` 不稳 | protoc 改从 Maven Central 下载（`protoc-4.35.1-linux-x86_64.exe` = protobuf v35.1） |
| bindgen 报 `Unable to find libclang` | kcp-sys 的 build script 用 bindgen，runner 缺 libclang | `LIBCLANG_PATH` 指向 NDK `toolchains/llvm/prebuilt/linux-x86_64/lib64` |
| bindgen 报 `bits/libc-header-start.h not found` | 没传 sysroot，clang 回落解析宿主机 glibc 头文件 | `BINDGEN_EXTRA_CLANG_ARGS=--sysroot=<NDK>/.../sysroot`（GitHub hosted runner 自带 libclang/build-essential，无需这两项） |
| 构建中途 `context canceled`（docker.sock） | runner 基础设施抖动 | 重试；健康检查 runner |

### 2.5 工作流结构层

| 症状 | 根因 | 修复 |
| --- | --- | --- |
| gui.yml 运行名显示为文件路径、0 job、立刻失败 | `if` 条件里使用了 `secrets` 上下文（GitHub 校验不允许） | 先把 secrets 求值为 job 级 env 布尔（如 `APPLE_CERT_CONFIGURED: ${{ secrets.APPLE_CERTIFICATE != '' }}`），`if` 里引用 env |
| `-result` 汇总 job 空 job 假失败（0 步骤、无 runner、秒失败） | GitHub 平台偶发 + 汇总 job 本身无价值 | 删除 `gui-result/mobile-result/core-result`（GitHub 已聚合 job 结果） |
| `pre_job`（skip-duplicate-actions）假失败 | 第三方 action 依赖 GitHub API，偶发失败 | 删除 pre_job 层（`concurrency.cancel-in-progress` 已防重复） |
| 一个矩阵目标失败，其余全部 cancelled | `fail-fast: true` | 打包矩阵改 `fail-fast: false` |

### 2.6 GitHub 账号 / 配额层

| 症状 | 根因 | 处理 |
| --- | --- | --- |
| 全部工作流 job `runner=''`、`steps=0`、2-3 秒内失败，连续多轮，状态页正常 | 私有仓库免费 Actions 分钟额度（2000 分钟/月）耗尽 | 开启付费 / 等额度重置 / 仓库设为 public（公开仓库无限分钟） |

## 3. 提升 CI/CD 成功率的优化清单

1. **工具链**：`rust-toolchain.toml` 钉死 1.95；工作流里 `dtolnay/rust-toolchain` 显式 `toolchain: 1.95` + `components: rustfmt, clippy`。
2. **pnpm 一致性**：CI 用 pnpm 10；任何依赖/overrides 变更后用 `npx -y pnpm@10.34.5 install --no-frozen-lockfile --lockfile-only` 更新 lockfile；overrides 只写在 `pnpm-workspace.yaml`。
3. **Android 版本固定**：cmdline-tools `12266719`、build-tools `34.0.0`、NDK `26.0.10792818`、platform `android-34`。
4. **下载源**：优先 Maven Central（protoc/protobuf-java）；GitHub release 附件在自托管环境不可靠；不要把 Gitea token 传给 GitHub 侧 action。
5. **gen/android 与 identifier 同步**：改 `tauri.conf.json` identifier 后执行 `pnpm tauri android init` 并提交整个 `gen/android`。
6. **typecheck 前置构建**：依赖 workspace JS 包（如 vpnservice-api）时先 `pnpm -r build`。
7. **bindgen 类 crate**：GitHub hosted runner 无需处理；自托管 runner 需 `LIBCLANG_PATH` + `BINDGEN_EXTRA_CLANG_ARGS=--sysroot=<NDK sysroot>`。
8. **machine-uid**：保持 `cfg(not(target_os = "android"))` 门控，Android 上回退随机/配置 ID。
9. **不要用 `secrets` 写 `if`**：一律先转 env 布尔。
10. **矩阵**：打包矩阵 `fail-fast: false`。
11. **删冗余 job**：`pre_job`（skip-duplicate）与 `-result` 汇总 job 已全部移除；test.yml 若保留建议同步清理。
12. **macOS 签名**：未配置 `APPLE_*` secrets 时自动出未签名 DMG；配置后自动签名 + notarize + staple。
13. **配额管理**：
    - 全量 GUI 一轮 ≈ 7 个 job × 40-60 分钟，mobile ≈ 4 × 15-25 分钟，一次全量约消耗 **400-500 分钟**；
    - 开发期用 `workflow_dispatch` 按需打包，或把 push 触发收敛到 `releases/**`；
    - 定期检查 **Settings → Billing and plans → Actions**；私有仓库额度告急时考虑转 public。
14. **test.yml 现状**：`cargo fmt --all -- --check` 对上游代码不通过（rustfmt 1.95 格式差异），需执行 `cargo fmt --all` 并提交，或按团队约定调整该 gate；`cargo hack`/clippy 需在稳定环境跑通后再放开。

## 4. 工作流现状速览

| 文件 | Job | 产物/作用 | 注意 |
| --- | --- | --- | --- |
| `gui.yml` | build-gui（7 平台矩阵） | NSIS exe / DMG / deb/rpm/AppImage | mac 无证书自动未签名 |
| `mobile.yml` | build-mobile（4 ABI） | APK ×4 | 依赖 gen/android 已提交 |
| `core.yml` | build_web / build（linux×4）/ build_magisk | 服务端二进制 + magisk 模块 | 产物名 `anf-easytier-core/cli` |
| `release.yml` | release | draft release 汇总 | 手动填 core/gui/mobile run id + 版本 |
| `test.yml` | check / pre-test / test_matrix | fmt/clippy/单测 | 格式 gate 需先修上游代码 |
| `docker.yml` / `nix.yml` / `ohos.yml` | — | 镜像 / nix / 鸿蒙 | 非本目标范围，按需维护 |

## 5. 常见故障速查表

| 症状 | 直接原因 | 处理 |
| --- | --- | --- |
| 运行名是 `.github/workflows/xxx.yml` 且 0 job | YAML 校验失败（语法、引用不存在的 job、secrets 进 if） | 本地 `python -c "import yaml; yaml.safe_load(open('...'))"` 校验；检查 `needs` 引用 |
| job 无 runner、0 步骤、秒失败 | GitHub 账号级限制（多为私有仓库额度耗尽） | 查 Billing → Actions；付费/公开仓库 |
| `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH` | lockfile 与 pnpm 版本/overrides 不一致 | 用 CI 同款 pnpm 重新生成 lockfile |
| `Command failed with exit code 101`（cargo） | 依赖某 crate 在目标平台不可编译 | 按报错定位 crate，参考 2.3 的 machine-uid 门控做法 |
| `Could not compile ...` 且是 bindgen/libclang | 自托管环境缺 libclang/sysroot | 见 2.4 |
| Compress 产物为空 | 二进制名与工作流里 glob 不一致 | 同步 `anf-easytier-*` 命名 |

## 6. 提交前检查清单

- [ ] 改了 Cargo.toml → `cargo update` 并提交 Cargo.lock
- [ ] 改了 pnpm 依赖/overrides → 用 pnpm 10 重新生成 lockfile
- [ ] 改了 Tauri identifier → `tauri android init` 并提交 gen/android
- [ ] 新增 Rust 依赖 → 确认 Android target 可编译（尤其无 android 实现的 crate）
- [ ] 改了工作流 → 本地 YAML 校验；`if` 里不用 secrets；矩阵用 `fail-fast: false`
- [ ] 推送前评估 Actions 配额余量
