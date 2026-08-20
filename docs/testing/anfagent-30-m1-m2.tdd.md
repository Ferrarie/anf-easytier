# ANFAGENT-30 M1/M2 TDD 证据报告

> 日期：2026-08-21
> 分支：`codex/anfagent-30`
> 测试命令：`cargo test -p easytier-web`（VM：Ubuntu 26.04，Rust 1.95）

## 1. 源计划

- 方案确认稿：docs/2026-08-20-anfagent-30-easytier-centralization.md
- M1 设计规格：docs/anfagent-30-m1.md
- M2 设计规格：docs/anfagent-30-m2.md

## 2. 用户旅程

1. 管理员 SSH 登录服务器，执行 `easytier-web admin-bind --machine-id <uuid>` 绑定自己的设备为管理员（初始 admin 可用 `--create-user-password` 引导）；
2. 管理员在 web 生成邀请码（限次/过期）；
3. 新设备凭“中心地址 + 邀请码”注册 → 进入待审批，只心跳、不获网络配置；
4. 管理员在 web 放行并分配 tag/网络后，设备才能入网；
5. 管理员创建网络实例与 tag，给设备分配，编写 ACL 规则（源 tag → 目标 tag + 协议/端口 + 动作），未命中默认拒绝；
6. 拒绝/踢出的设备失去授权，重复注册待审设备回到 pending。

## 3. 任务报告（RED → GREEN）

### M1 数据层（db/anf.rs）

- 用例与实现同批落地后，经编译修正（PaginatorTrait/IntoActiveModel 导入、move 借用）后 GREEN：`cargo test -p easytier-web anf::` → 13 passed。
- 冒烟：`easytier-web --db /tmp/anf-test.db admin-bind --machine-id ... --username admin --create-user-password ...` 成功，SQLite 校验：admin 进入 superusers 组、设备 approved、显示名=机器码前 8 位。

### M1 REST/CLI/会话层

- 编译门：`cargo check -p easytier-web` GREEN（修复 `#[async_trait]` FromRequestParts、AuthUser 导入、handler 工厂简化为具名函数）；
- 全量回归：`cargo test -p easytier-web` → 94 passed（原 81 + M1 13）。

### M2 数据层与编译（db/anf_networks.rs）

- 用例与实现落地后 GREEN：`cargo test -p easytier-web anf_networks::` → 8 passed；
- 全量回归：`cargo test -p easytier-web` → 103 passed（M2 新增 9：含 update_acl_rule）。

### 前端（M1/M2）

- 质量门：`pnpm build`（frontend-lib codegen + vue-tsc -b + vite build）GREEN；
- 过程修复：props.api 可选链导致的 TS 类型错误（改为 `?? []` / async 闭包）。

## 4. 测试规格表

| # | 保证内容 | 位置 | 类型 | 结果 |
|---|---------|------|------|------|
| 1 | 邀请码唯一 12 字符、初始可用 | db/anf.rs generate_invite_produces_unique_12_char_codes | 单元 | PASS |
| 2 | 一次性邀请码用尽拒绝 | invite_consumed_once_then_rejected | 单元 | PASS |
| 3 | 过期/吊销邀请码拒绝 | invite_expired_rejected / disabled_invite_rejected | 单元 | PASS |
| 4 | 注册默认 pending、显示名=机器码前缀 | register_device_defaults_to_pending_with_prefix_name | 单元 | PASS |
| 5 | 重复注册不降级已放行设备 | re_register_pending_updates_but_approved_stays | 单元 | PASS |
| 6 | 设备状态机（含 rejected 终态） | device_status_machine_transitions / rejected_is_terminal | 单元 | PASS |
| 7 | admin-bind 幂等、设备 approved、用户进 superusers | admin_bind_is_idempotent_and_approves_device | 单元 | PASS |
| 8 | 设备授权判定：未登记放行、pending/rejected 不放行 | unknown_device_stays_legacy_authorized / pending_or_rejected_device_is_not_authorized | 单元 | PASS |
| 9 | tag/网络全量替换 | update_device_replaces_tags_and_networks | 单元 | PASS |
| 10 | 网络删除保护（被设备使用 409） | anf_networks.rs network_crud_and_delete_protection | 单元 | PASS |
| 11 | tag 删除保护 | tag_crud_and_delete_protection | 单元 | PASS |
| 12 | ACL 规则输入校验（协议/动作/tag 名/优先级） | acl_rule_validation | 单元 | PASS |
| 13 | ACL 编译默认拒绝（chain default_action=drop） | compile_default_deny_without_rules | 单元 | PASS |
| 14 | group declares=全量 tag、members=本设备 tag | compile_declares_all_tags_and_members_only_device_tags | 单元 | PASS |
| 15 | 规则按优先级降序、字段映射（协议/端口/动作/groups） | compile_rules_sorted_by_priority_desc_with_fields_mapped | 单元 | PASS |
| 16 | 禁用规则忽略、网络外设备不参与 | compile_ignores_disabled_rules_and_out_of_network_devices | 单元 | PASS |
| 17 | 更新规则全量替换、缺失报错 | update_acl_rule_replaces_fields | 单元 | PASS |

## 5. 覆盖与已知缺口

- 覆盖率未单独跑（上游未配置 llvm-cov）；以“核心逻辑全分支测试 + 全量回归 103/103”为当前门禁，建议 M4 前补覆盖率工具；
- 会话授权接线（client_manager/session.rs 心跳按 devices.status 判授权）未做自动化单测（需完整 RPC harness），由 DB 层 device_is_authorized 用例覆盖逻辑、M3 端到端覆盖整链路；webhook 模式未接入设备状态（已知缺口，二期处理）；
- REST 层沿用上游无 HTTP 测试的现状，M3 用 docker compose + curl 做端到端验收；
- 前端无 Playwright E2E；以 vue-tsc + vite 构建为门禁，浏览器交互验收归入 M3/M4；
- 邀请码熵：当前 12 位十六进制（48 bit），内部自用可接受；对外或加强建议升级 16 位 base32（记入 M4 安全加固）；
- easytier 主 crate 的 TUN/netns 集成测试需 root 运行（本 VM 已验证 root 下通过），非本次改动引入。

## 6. 检查点提交（codex/anfagent-30）

- 68a79e85：M1 后端（含 13 用例）
- bf82de0f：M1 前端（构建通过）
- 01acd45b：M2（含 9 用例，103/103）
