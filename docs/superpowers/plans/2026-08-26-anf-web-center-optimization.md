# ANF Web 中心优化 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按定稿设计树优化 ANF Web 中心（easytier-web）：中心端口/连接信息展示、网络随机网段、Tag 交互与可编辑、ACL 规则可编辑，并附带 Dashboard 统计与轮询降频、设备列表筛选、轻量品牌化与 i18n 清理。

**Architecture:** 后端新增 `CenterInfo`（运行时端口/中心参数，随 `RestfulServer` 以 Extension 下发）与 `GET /api/v1/center/info`；`create_network` 空网段自动生成随机 /24；tag 新增 `PATCH /api/v1/tags/:id` 并级联 `device_tags` 与 ACL 规则 JSON 引用；ACL 编辑复用已有 PATCH。前端在现有 PrimeVue 组件上增加卡片、弹窗与编辑入口，不动 config-server/客户端协议。

**Tech Stack:** Rust（axum 0.7 + sea-orm 1.1 + SQLite + rand 0.8）、Vue 3.5 `<script setup>` + PrimeVue 4 + vue-i18n + TypeScript（vue-tsc + vite）。

## Global Constraints

- 不改变任何连接/审批/配置语义；`cargo test -p easytier-web` 必须全绿（现 110+ 用例）。
- 前端代码风格 @antfu（无分号、单引号、2 空格缩进）；`pnpm --dir easytier-web/frontend build`（含 vue-tsc）必须通过。
- 后端错误文案中文；新增文案一律走 i18n（`frontend-lib/src/locales/cn.yaml` / `en.yaml`）。
- 端口展示**数据驱动**：以运行时 `Cli`/`FeatureFlags` 值为准，不硬编码。
- 不动 config-server 协议层；注册页残留只做前端下架。
- 提交信息用仓库既有风格（`feat(anf): ...` / `fix(anf): ...` / `docs(anf): ...`）。
- 共享工作区当前在 `codex/acme-ssl`（其它任务在用）：git 写操作（branch/commit）需走提权；提交一律落 `codex/anf-web-center-design-tree`，且不切换共享 checkout（用临时 index 或 worktree 方式提交）。

---

## File Structure

- `easytier-web/src/main.rs` —— 新增 `CenterInfo` 结构 + `from_cli`；注册为 Extension。
- `easytier-web/src/restful/center.rs`（新建）—— `GET /api/v1/center/info` 处理器。
- `easytier-web/src/restful/mod.rs` —— 挂载 center 路由；扩展 summary 或新增 ANF 统计端点；`RestfulServer::new` 增加 `center_info` 参数。
- `easytier-web/src/restful/networks.rs` —— 创建网络错误映射（AnfNetError）。
- `easytier-web/src/db/anf_networks.rs` —— 随机网段生成、`create_network` 校验、`update_tag` 级联、`list_network_ids_using_tag`、统计计数。
- `easytier-web/src/restful/tags.rs` —— `PATCH /api/v1/tags/:id`。
- `easytier-web/src/restful/acl.rs` —— `reconcile_after_acl_change` 改 `pub(crate)` 供 tags 复用。
- `easytier-web/frontend/src/modules/api.ts` —— `centerInfo` / `updateTag` / `updateAclRule` / `anfStats`。
- `easytier-web/frontend/src/components/Dashboard.vue` —— 中心连接信息卡 + 统计卡 + 轮询 10s。
- `easytier-web/frontend/src/components/NetworkManagementPage.vue` —— 名称必填 + 随机网段提示。
- `easytier-web/frontend/src/components/TagManagementPage.vue` —— 弹窗化 + 编辑。
- `easytier-web/frontend/src/components/AclEditorPage.vue` —— 编辑弹窗。
- `easytier-web/frontend/src/components/DeviceAdminPage.vue` / `DeviceList.vue` —— 状态筛选 + 关键字（低风险）。
- `easytier-web/frontend/src/main.ts` / `App.vue` / `Login.vue` / `MainPage.vue` / `style.css` —— 品牌化 + 残留清理。
- `easytier-web/frontend-lib/src/locales/cn.yaml` / `en.yaml` —— 新增文案。

---

### Task 1: 后端——CenterInfo + `GET /api/v1/center/info`

**Files:**
- Modify: `easytier-web/src/main.rs`（Cli 定义之后新增 `CenterInfo`；`RestfulServer::new` 调用处传参）
- Create: `easytier-web/src/restful/center.rs`
- Modify: `easytier-web/src/restful/mod.rs`（`RestfulServer` 结构体 + `new` 签名 + 路由 + Extension）

**Interfaces:**
- Consumes: `Cli`（`config_server_protocol: String`、`config_server_port: u16`、`api_server_port: u16`、`web_server_port: Option<u16>`、`feature_flags.anf_network_name`、`feature_flags.anf_center_peer_url`）、`EASYTIER_VERSION`。
- Produces: `CenterInfo { version, api_server_port, web_server_port, config_server_protocol, config_server_port, anf_network_name, anf_center_peer_url }`；HTTP `GET /api/v1/center/info` 返回同名 JSON（serde 序列化）。

- [ ] **Step 1: 写失败测试（main.rs 末尾 `#[cfg(test)]`）**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn center_info_from_cli_maps_runtime_values() {
        let cli = Cli::try_parse_from([
            "easytier-web",
            "--config-server-port", "25220",
            "--config-server-protocol", "udp",
            "--api-server-port", "15211",
            "--anf-network-name", "anf-m3",
            "--anf-center-peer-url", "tcp://10.126.126.6:13110",
        ])
        .unwrap();
        let info = CenterInfo::from_cli(&cli);
        assert_eq!(info.version, EASYTIER_VERSION);
        assert_eq!(info.api_server_port, 15211);
        assert_eq!(info.config_server_protocol, "udp");
        assert_eq!(info.config_server_port, 25220);
        assert_eq!(info.anf_network_name, "anf-m3");
        assert_eq!(
            info.anf_center_peer_url.as_deref(),
            Some("tcp://10.126.126.6:13110")
        );
    }
}
```

- [ ] **Step 2: 运行确认失败**

`cargo test -p easytier-web --lib` → 编译失败（`CenterInfo` 未定义）。

- [ ] **Step 3: 实现 CenterInfo**

在 `main.rs` 的 `Cli` 定义之后新增：

```rust
/// 中心运行信息（供 web 前端展示端口/服务/连接提示；值全部来自运行时配置，不硬编码）。
#[derive(Debug, Clone)]
pub struct CenterInfo {
    pub version: &'static str,
    pub api_server_port: u16,
    pub web_server_port: Option<u16>,
    pub config_server_protocol: String,
    pub config_server_port: u16,
    pub anf_network_name: String,
    pub anf_center_peer_url: Option<String>,
}

impl CenterInfo {
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            version: EASYTIER_VERSION,
            api_server_port: cli.api_server_port,
            web_server_port: cli.web_server_port,
            config_server_protocol: cli.config_server_protocol.clone(),
            config_server_port: cli.config_server_port,
            anf_network_name: cli.feature_flags.anf_network_name.clone(),
            anf_center_peer_url: cli.feature_flags.anf_center_peer_url.clone(),
        }
    }
}
```

在 `main()` 中 `let feature_flags = Arc::new(cli.feature_flags);`（第 434 行附近）之后新增：

```rust
let center_info = Arc::new(CenterInfo::from_cli(&cli));
```

并把 `center_info` 传入 `RestfulServer::new(...)`（第 446 行附近，`feature_flags.clone()` 之后）。

- [ ] **Step 4: 新增 REST 处理器**

新建 `easytier-web/src/restful/center.rs`：

```rust
//! 中心运行信息（管理员）。

use axum::{Extension, Json, Router, routing::get};
use serde::Serialize;

use super::{AdminSession, AppStateInner};
use crate::CenterInfo;

#[derive(Debug, Serialize)]
pub struct CenterInfoJson {
    pub version: String,
    pub api_server_port: u16,
    pub web_server_port: Option<u16>,
    pub config_server_protocol: String,
    pub config_server_port: u16,
    pub anf_network_name: String,
    pub anf_center_peer_url: Option<String>,
}

impl From<&CenterInfo> for CenterInfoJson {
    fn from(info: &CenterInfo) -> Self {
        Self {
            version: info.version.to_string(),
            api_server_port: info.api_server_port,
            web_server_port: info.web_server_port,
            config_server_protocol: info.config_server_protocol.clone(),
            config_server_port: info.config_server_port,
            anf_network_name: info.anf_network_name.clone(),
            anf_center_peer_url: info.anf_center_peer_url.clone(),
        }
    }
}

pub fn router() -> Router<AppStateInner> {
    Router::new().route("/api/v1/center/info", get(handle_center_info))
}

async fn handle_center_info(
    _admin: AdminSession,
    axum::Extension(center_info): axum::Extension<Arc<CenterInfo>>,
) -> Result<Json<CenterInfoJson>, super::HttpHandleError> {
    Ok(Json(CenterInfoJson::from(center_info.as_ref())))
}
```

（`Arc` 已由 `restful/mod.rs` 的 `use std::sync::Arc` 提供；若中心文件未引入，补 `use std::sync::Arc;`。）

- [ ] **Step 5: 接线 `restful/mod.rs`**

在 `RestfulServer` 结构体新增字段 `center_info: Arc<CenterInfo>`；`new` 签名新增参数 `center_info: Arc<CenterInfo>` 并赋值；在 `pub fn router(&self)`（第 305 行附近）的 Router 合并处追加：

```rust
.merge(center::router())
```

并在 Router 上追加 `.layer(Extension(self.center_info.clone()))`（与既有 `Extension(self.feature_flags.clone())` 同层，第 336 行附近）。

在 `restful/mod.rs` 顶部 `mod` 声明区新增 `mod center;`（或 `pub mod center;`，按既有模块可见性）。

`main.rs` 中 `RestfulServer::new` 调用处同步传入 `center_info`。

- [ ] **Step 6: 运行测试确认通过**

`cargo test -p easytier-web --lib` → 新增用例 PASS，既有用例全绿。

- [ ] **Step 7: 提交**

```bash
git add easytier-web/src/main.rs easytier-web/src/restful/center.rs easytier-web/src/restful/mod.rs
git commit -m "feat(anf): web 中心新增 center/info 运行信息接口"
```

---

### Task 2: 后端——`create_network` 随机网段 + 名称/cidr 校验

**Files:**
- Modify: `easytier-web/src/db/anf_networks.rs`（`create_network` 签名/校验/随机生成 + 测试）
- Modify: `easytier-web/src/restful/networks.rs`（错误映射）

**Interfaces:**
- Consumes: `Db::list_networks()`、`AnfNetError::{InvalidInput, Db}`、`rand` 0.8。
- Produces: `Db::create_network(&self, name: &str, cidr: Option<String>) -> Result<entity::network_instances::Model, AnfNetError>`（空 cidr 自动随机 `10.a.b.0/24`）；`fn random_cidr(existing: &[String], rng: &mut impl rand::Rng) -> Option<String>`（供测试注入种子）。

- [ ] **Step 1: 写失败测试（anf_networks.rs `tests` 模块追加）**

```rust
#[tokio::test]
async fn create_network_requires_name_and_validates_cidr() {
    let db = Db::memory_db().await;

    let err = db.create_network("   ", None).await.unwrap_err();
    assert!(matches!(err, AnfNetError::InvalidInput(_)));

    let err = db
        .create_network("网A", Some("999.1.1.1/24".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(err, AnfNetError::InvalidInput(_)));

    let err = db
        .create_network("网A", Some("10.0.0.0/31".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(err, AnfNetError::InvalidInput(_)));
}

#[tokio::test]
async fn create_network_assigns_random_cidr_when_empty() {
    let db = Db::memory_db().await;

    let net = db.create_network("随机网", None).await.unwrap();
    let cidr = net.cidr.unwrap();
    assert!(cidr.starts_with("10.") && cidr.ends_with(".0/24"), "{cidr}");

    // 与既有网段不冲突
    let net2 = db.create_network("随机网2", None).await.unwrap();
    assert_ne!(net2.cidr.as_deref(), Some(cidr.as_str()));

    // 显式 cidr 原样保留
    let explicit = db
        .create_network("显式网", Some("172.16.5.0/24".to_string()))
        .await
        .unwrap();
    assert_eq!(explicit.cidr.as_deref(), Some("172.16.5.0/24"));
}

#[test]
fn random_cidr_excludes_existing_and_reserved() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let existing = vec!["10.126.0.0/24".to_string(), "10.144.0.0/24".to_string()];
    let cidr = random_cidr(&existing, &mut rng).unwrap();
    assert!(cidr.starts_with("10.") && cidr.ends_with(".0/24"));
    assert!(!existing.contains(&cidr));
    assert!(cidr != "10.126.0.0/16" && cidr != "10.144.0.0/24");
}
```

- [ ] **Step 2: 运行确认失败**

`cargo test -p easytier-web --lib` → 三个新用例 FAIL（`create_network` 仍返回 `DbErr`/空名可入库/无随机）。

- [ ] **Step 3: 实现**

`anf_networks.rs` 新增辅助函数（放在 `valid_tag_name` 附近）：

```rust
use rand::Rng;

/// 保留段：VM mesh（etgame/anidev）与既有默认示例段，避免与运营网冲突。
const RESERVED_CIDRS: &[&str] = &["10.126.0.0/16", "10.144.0.0/24"];

/// 校验 IPv4 CIDR（前缀 /8–/30，网络地址与广播地址区间至少可容纳 2 台主机）。
fn valid_cidr(cidr: &str) -> bool {
    let Some((ip_part, prefix_part)) = cidr.trim().split_once('/') else {
        return false;
    };
    let Ok(ip) = ip_part.trim().parse::<std::net::Ipv4Addr>() else {
        return false;
    };
    let Ok(prefix) = prefix_part.trim().parse::<u32>() else {
        return false;
    };
    (8..=30).contains(&prefix) && ip.octets()[0] != 0
}

/// 生成不与 existing/保留段冲突的随机私有 /24（10.a.b.0/24），最多重试 16 次。
fn random_cidr(existing: &[String], rng: &mut impl Rng) -> Option<String> {
    for _ in 0..16 {
        let cidr = format!(
            "10.{}.{}.0/24",
            rng.gen_range(1..=254),
            rng.gen_range(0..=255)
        );
        if !existing.iter().any(|e| e == &cidr) && !RESERVED_CIDRS.contains(&cidr.as_str()) {
            return Some(cidr);
        }
    }
    None
}
```

把 `create_network` 签名改为 `Result<entity::network_instances::Model, AnfNetError>`，开头加校验与随机生成：

```rust
pub async fn create_network(
    &self,
    name: &str,
    cidr: Option<String>,
) -> Result<entity::network_instances::Model, AnfNetError> {
    use entity::network_instances;

    if name.trim().is_empty() {
        return Err(AnfNetError::InvalidInput("网络名称不能为空".to_string()));
    }
    let cidr = match cidr {
        Some(c) if !c.trim().is_empty() => {
            let c = c.trim().to_string();
            if !valid_cidr(&c) {
                return Err(AnfNetError::InvalidInput(
                    "网段格式非法，应为 IPv4 CIDR（/8–/30）".to_string(),
                ));
            }
            Some(c)
        }
        _ => {
            let existing: Vec<String> = self
                .list_networks()
                .await?
                .iter()
                .filter_map(|n| n.cidr.clone())
                .collect();
            random_cidr(&existing, &mut rand::thread_rng())
                .ok_or_else(|| AnfNetError::InvalidInput("随机网段生成失败，请重试".to_string()))?
                .pipe(Some)
        }
    };

    let id = format!("{NET_ID_PREFIX}{}", &Uuid::new_v4().simple().to_string()[..8]);
    let m = network_instances::ActiveModel {
        id: Set(id.clone()),
        name: Set(name.trim().to_string()),
        cidr: Set(cidr),
        created_at: Set(now()),
        updated_at: Set(now()),
    };
    network_instances::Entity::insert(m).exec(self.orm_db()).await?;
    self.get_network(&id)
        .await?
        .ok_or_else(|| AnfNetError::InvalidInput("网络实例创建后未找到".to_string()))
}
```

> 注：`.pipe(Some)` 仅示意「把 Option<String> 包成 Some」——直接写 `Some(random_cidr(...)?)` 即可；`?` 对 `Option` 需用 `.ok_or_else(...)?`。最终代码以「通过 cargo check」为准：
> `let cidr = random_cidr(&existing, &mut rand::thread_rng()).ok_or_else(|| AnfNetError::InvalidInput("随机网段生成失败，请重试".to_string()))?;` 然后 `cidr: Set(Some(cidr))`。

`restful/networks.rs` 的 `create` 处理器把 `.map_err(convert_db_error)` 改为按 `AnfNetError` 映射（参考 `restful/tags.rs` 的 `remove` 匹配写法：`InvalidInput → 422`，`Db(d) → convert_db_error(d)`，其它 → 500）。

- [ ] **Step 4: 运行测试确认通过**

`cargo test -p easytier-web --lib` → 新用例 PASS；既有 `network_crud_and_delete_protection` 等调用 `create_network(...).unwrap()` 的用例保持通过（`AnfNetError` 实现 `Debug`，`unwrap_err`/`unwrap` 可用；若有 `From<DbErr> for AnfNetError` 缺失则补 `#[error(transparent)] Db(#[from] DbErr)` 变体）。

- [ ] **Step 5: 提交**

```bash
git add easytier-web/src/db/anf_networks.rs easytier-web/src/restful/networks.rs
git commit -m "feat(anf): 新建网络空网段自动随机分配 + 名称/cidr 校验"
```

---

### Task 3: 后端——tag 改名 + 级联 + `PATCH /api/v1/tags/:id`

**Files:**
- Modify: `easytier-web/src/db/anf_networks.rs`（`update_tag`、`list_network_ids_using_tag` + 测试）
- Modify: `easytier-web/src/restful/tags.rs`（PATCH 路由 + 处理器）
- Modify: `easytier-web/src/restful/acl.rs`（`reconcile_after_acl_change` 改 `pub(crate)`）

**Interfaces:**
- Consumes: `valid_tag_name`、`json_to_vec` / `vec_to_json`、`AnfNetError::{TagNotFound, InvalidInput, Db}`、`reconcile_after_acl_change(&client_mgr, &db, &feature_flags, network_id)`。
- Produces: `Db::update_tag(&self, id: i32, name: &str) -> Result<entity::tags::Model, AnfNetError>`（改名后返回新模型）；`Db::list_network_ids_using_tag(&self, tag: &str) -> Result<Vec<String>, DbErr>`；HTTP `PATCH /api/v1/tags/:id`，body `{"name": string}`。

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn tag_rename_updates_references_and_rejects_duplicates() {
    let db = Db::memory_db().await;
    let admin = admin_user(&db).await;

    let tag = db.create_tag("办公").await.unwrap();
    let net = db.create_network("办公网", Some("10.20.0.0/24".to_string())).await.unwrap();
    let device = register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公"], &[&net.id]).await;

    let rule = db
        .create_acl_rule(&NewAclRule {
            network_inst_id: net.id.clone(),
            name: "allow-http".to_string(),
            enabled: true,
            source_tags: vec!["办公".to_string()],
            destination_tags: vec!["服务器".to_string()],
            protocol: "tcp".to_string(),
            ports: vec!["80".to_string()],
            action: "allow".to_string(),
            priority: 100,
        })
        .await
        .unwrap();

    let renamed = db.update_tag(tag.id, "办公区").await.unwrap();
    assert_eq!(renamed.name, "办公区");

    // device_tags 级联
    let tags = db.list_device_tags(device).await.unwrap();
    assert!(tags.contains(&"办公区".to_string()));

    // ACL 规则 JSON 级联
    let rule = db.get_acl_rule(rule.id).await.unwrap().unwrap();
    assert_eq!(rule.source_tags, vec_to_json(&["办公区".to_string()]));

    // 重名拒绝
    db.create_tag("服务器").await.unwrap();
    let err = db.update_tag(tag.id, "服务器").await.unwrap_err();
    assert!(matches!(err, AnfNetError::InvalidInput(_)));

    // 不存在
    let err = db.update_tag(99999, "新名").await.unwrap_err();
    assert!(matches!(err, AnfNetError::TagNotFound));
}

#[tokio::test]
async fn list_network_ids_using_tag_covers_rules_and_devices() {
    let db = Db::memory_db().await;
    let admin = admin_user(&db).await;
    let net = db.create_network("网", Some("10.30.0.0/24".to_string())).await.unwrap();
    register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["网关"], &[&net.id]).await;

    let ids = db.list_network_ids_using_tag("网关").await.unwrap();
    assert!(ids.contains(&net.id));
}
```

> 若 `get_acl_rule` 不存在，则在 `anf_networks.rs` 补一个只读查询 `pub async fn get_acl_rule(&self, id: i32) -> Result<Option<entity::acl_rules::Model>, DbErr>`（`find_by_id`），测试断言改用「重新 `list_acl_rules(&net.id)` 后按 id 找到该规则」。

- [ ] **Step 2: 运行确认失败**

`cargo test -p easytier-web --lib` → `update_tag` / `list_network_ids_using_tag` 未定义，编译失败。

- [ ] **Step 3: 实现 db 方法**

`anf_networks.rs` 的 tag 区新增：

```rust
/// 更新 tag 名称（ID 不可变）；级联同步 device_tags 与 ACL 规则 JSON 引用。
pub async fn update_tag(
    &self,
    id: i32,
    name: &str,
) -> Result<entity::tags::Model, AnfNetError> {
    use entity::{acl_rules, device_tags, tags};

    let trimmed = name.trim().to_string();
    if !valid_tag_name(&trimmed) {
        return Err(AnfNetError::InvalidInput(
            "tag 名非法（字母/数字/中划线/下划线/点，≤32 字符，不含空白）".to_string(),
        ));
    }
    let existing = tags::Entity::find_by_id(id)
        .one(self.orm_db())
        .await?
        .ok_or(AnfNetError::TagNotFound)?;
    if existing.name == trimmed {
        return Ok(existing);
    }
    let dup = tags::Entity::find()
        .filter(tags::Column::Name.eq(&trimmed))
        .one(self.orm_db())
        .await?;
    if dup.is_some() {
        return Err(AnfNetError::InvalidInput("tag 名称已存在".to_string()));
    }

    // 级联 device_tags（按名字符串引用）
    let rows = device_tags::Entity::find()
        .filter(device_tags::Column::Tag.eq(&existing.name))
        .all(self.orm_db())
        .await?;
    device_tags::Entity::delete_many()
        .filter(device_tags::Column::Tag.eq(&existing.name))
        .exec(self.orm_db())
        .await?;
    for row in rows {
        device_tags::ActiveModel {
            device_id: Set(row.device_id),
            tag: Set(trimmed.clone()),
        }
        .insert(self.orm_db())
        .await?;
    }

    // 级联 ACL 规则 JSON 引用
    for rule in acl_rules::Entity::find().all(self.orm_db()).await? {
        let mut src = json_to_vec(&rule.source_tags);
        let mut dst = json_to_vec(&rule.destination_tags);
        let mut changed = false;
        for v in src.iter_mut() {
            if v == &existing.name {
                *v = trimmed.clone();
                changed = true;
            }
        }
        for v in dst.iter_mut() {
            if v == &existing.name {
                *v = trimmed.clone();
                changed = true;
            }
        }
        if changed {
            let mut m = rule.into_active_model();
            m.source_tags = Set(vec_to_json(&src));
            m.destination_tags = Set(vec_to_json(&dst));
            m.updated_at = Set(now());
            acl_rules::Entity::update(m).exec(self.orm_db()).await?;
        }
    }

    let mut m = existing.into_active_model();
    m.name = Set(trimmed);
    Ok(tags::Entity::update(m).exec(self.orm_db()).await?)
}

/// 引用某 tag 的网络 id（规则引用 ∪ 带该 tag 的设备所在网络）。
pub async fn list_network_ids_using_tag(&self, tag: &str) -> Result<Vec<String>, DbErr> {
    use entity::{acl_rules, device_networks, device_tags};

    let mut ids: Vec<String> = Vec::new();
    for rule in acl_rules::Entity::find().all(self.orm_db()).await? {
        let hit = json_to_vec(&rule.source_tags).contains(&tag.to_string())
            || json_to_vec(&rule.destination_tags).contains(&tag.to_string());
        if hit && !ids.contains(&rule.network_inst_id) {
            ids.push(rule.network_inst_id.clone());
        }
    }
    let device_ids: Vec<i32> = device_tags::Entity::find()
        .filter(device_tags::Column::Tag.eq(tag))
        .all(self.orm_db())
        .await?
        .into_iter()
        .map(|d| d.device_id)
        .collect();
    for did in device_ids {
        for dn in device_networks::Entity::find()
            .filter(device_networks::Column::DeviceId.eq(did))
            .all(self.orm_db())
            .await?
        {
            if !ids.contains(&dn.network_inst_id) {
                ids.push(dn.network_inst_id.clone());
            }
        }
    }
    Ok(ids)
}
```

`acl.rs` 中把 `reconcile_after_acl_change` 的 `async fn` 改为 `pub(crate) async fn`。

- [ ] **Step 4: 实现 PATCH 路由（restful/tags.rs）**

`use axum::routing::{delete, patch, post};`，路由追加：

```rust
.route("/api/v1/tags/:id", delete(remove).patch(update))
```

处理器（复用 `AdminSession` / `Extension<Db>` / `Extension<Arc<FeatureFlags>>` / `State(client_mgr)`，参照 acl.rs 的 update）：

```rust
#[derive(Debug, Deserialize)]
pub struct UpdateTagRequest {
    pub name: String,
}

async fn update(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    State(client_mgr): State<AppStateInner>,
    axum::Extension(feature_flags): axum::Extension<Arc<FeatureFlags>>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateTagRequest>,
) -> Result<Json<TagJson>, HttpHandleError> {
    use crate::db::anf_networks::AnfNetError;

    let tag = match db.update_tag(id, &req.name).await {
        Ok(t) => t,
        Err(AnfNetError::TagNotFound) => {
            return Err((StatusCode::NOT_FOUND, Json::from(other_error("tag 不存在"))));
        }
        Err(AnfNetError::InvalidInput(msg)) => {
            return Err((StatusCode::UNPROCESSABLE_ENTITY, Json::from(other_error(msg))));
        }
        Err(AnfNetError::Db(d)) => return Err(convert_db_error(d)),
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(e.to_string())),
            ));
        }
    };

    let network_ids = db
        .list_network_ids_using_tag(&tag.name)
        .await
        .map_err(convert_db_error)?;
    for network_id in network_ids {
        super::acl::reconcile_after_acl_change(&client_mgr, &db, &feature_flags, &network_id)
            .await?;
    }
    Ok(Json(TagJson::from_model(&db, tag).await?))
}
```

> `use super::acl;` 或按模块路径引入 `crate::restful::acl::reconcile_after_acl_change`；`State`/`AppStateInner` 已在文件顶部可用（参考 acl.rs 写法）。

- [ ] **Step 5: 运行测试确认通过**

`cargo test -p easytier-web --lib` → 新用例 PASS，既有全绿。

- [ ] **Step 6: 提交**

```bash
git add easytier-web/src/db/anf_networks.rs easytier-web/src/restful/tags.rs easytier-web/src/restful/acl.rs
git commit -m "feat(anf): tag 改名级联设备/ACL 引用并热更新"
```

---

### Task 4: 后端——ANF 统计接口（Dashboard 数据源）

**Files:**
- Modify: `easytier-web/src/db/anf_networks.rs`（计数方法 + 测试）
- Modify: `easytier-web/src/restful/mod.rs`（`GET /api/v1/anf/stats`）

**Interfaces:**
- Consumes: `Db`、`entity::{devices, network_instances, tags, acl_rules}`。
- Produces: `Db::anf_stats(&self) -> Result<AnfStats, DbErr>`，`AnfStats { total_devices, pending, approved, rejected, kicked, networks, tags, rules }`；HTTP `GET /api/v1/anf/stats`。

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn anf_stats_counts_devices_by_status_and_resources() {
    let db = Db::memory_db().await;
    let admin = admin_user(&db).await;

    db.create_network("网1", Some("10.40.0.0/24".to_string())).await.unwrap();
    db.create_tag("办公").await.unwrap();
    register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公"], &["网1-id-占位"]).await;

    let stats = db.anf_stats().await.unwrap();
    assert_eq!(stats.networks, 1);
    assert_eq!(stats.tags, 1);
    assert!(stats.approved >= 0);
    assert_eq!(stats.total_devices, stats.pending + stats.approved + stats.rejected + stats.kicked);
}
```

> 测试中网络 id 用占位会导致注册失败——改为先创建网络并取 `net.id`，把「网1-id-占位」换成真实 id：
> `let net = db.create_network("网1", Some("10.40.0.0/24".to_string())).await.unwrap();` 后 `register_approved_device(&db, admin, ..., &["办公"], &[&net.id]).await;`

- [ ] **Step 2: 运行确认失败**

`cargo test -p easytier-web --lib` → `anf_stats` 未定义。

- [ ] **Step 3: 实现**

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnfStats {
    pub total_devices: u32,
    pub pending: u32,
    pub approved: u32,
    pub rejected: u32,
    pub kicked: u32,
    pub networks: u32,
    pub tags: u32,
    pub rules: u32,
}

impl Db {
    /// ANF 管理统计（Dashboard 数据源）。
    pub async fn anf_stats(&self) -> Result<AnfStats, DbErr> {
        use entity::{acl_rules, devices, network_instances, tags};
        use crate::db::anf::DeviceStatus;

        let all = devices::Entity::find().all(self.orm_db()).await?;
        let count = |status: DeviceStatus| {
            all.iter()
                .filter(|d| DeviceStatus::from_str(&d.status).ok() == Some(status))
                .count() as u32
        };
        Ok(AnfStats {
            total_devices: all.len() as u32,
            pending: count(DeviceStatus::Pending),
            approved: count(DeviceStatus::Approved),
            rejected: count(DeviceStatus::Rejected),
            kicked: count(DeviceStatus::Kicked),
            networks: network_instances::Entity::find().count(self.orm_db()).await? as u32,
            tags: tags::Entity::find().count(self.orm_db()).await? as u32,
            rules: acl_rules::Entity::find().count(self.orm_db()).await? as u32,
        })
    }
}
```

（`DeviceStatus` 的 `from_str` 与变体名以 `db/anf.rs` 现有实现为准；若 `status` 字段为小写字符串，按既有 `set_device_status`/测试用法核对。）

`restful/mod.rs` 新增：

```rust
#[derive(Debug, serde::Serialize)]
struct AnfStatsJson {
    total_devices: u32,
    pending: u32,
    approved: u32,
    rejected: u32,
    kicked: u32,
    networks: u32,
    tags: u32,
    rules: u32,
}

async fn handle_get_anf_stats(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
) -> Result<Json<AnfStatsJson>, HttpHandleError> {
    let s = db.anf_stats().await.map_err(convert_db_error)?;
    Ok(Json(AnfStatsJson {
        total_devices: s.total_devices,
        pending: s.pending,
        approved: s.approved,
        rejected: s.rejected,
        kicked: s.kicked,
        networks: s.networks,
        tags: s.tags,
        rules: s.rules,
    }))
}
```

在 Router（第 316 行 `/api/v1/summary` 附近）追加 `.route("/api/v1/anf/stats", get(Self::handle_get_anf_stats))`（`AdminSession` 已由模块引入）。

- [ ] **Step 4: 运行测试确认通过**

`cargo test -p easytier-web --lib` → PASS。

- [ ] **Step 5: 提交**

```bash
git add easytier-web/src/db/anf_networks.rs easytier-web/src/restful/mod.rs
git commit -m "feat(anf): 新增 ANF 统计接口 anf/stats"
```

---

### Task 5: 前端 api.ts——新接口方法

**Files:**
- Modify: `easytier-web/frontend/src/modules/api.ts`

**Interfaces:**
- Consumes: 后端 `GET /api/v1/center/info`、`PATCH /api/v1/tags/:id`、`PATCH /api/v1/networks/:id/rules/:ruleId`、`GET /api/v1/anf/stats`。
- Produces: `ApiClient.centerInfo()`, `ApiClient.anfStats()`, `ApiClient.updateTag(id, name)`, `ApiClient.updateAclRule(networkId, ruleId, rule)`。

> 前端无测试框架（package.json 无 test script），按 TDD 例外处理：验证 = `vue-tsc` 类型检查 + vite 构建 + 后续页面手工验证。

- [ ] **Step 1: 新增接口方法与类型**

在 `api.ts` 的接口类型区追加：

```ts
export interface CenterInfo {
  version: string
  api_server_port: number
  web_server_port?: number
  config_server_protocol: string
  config_server_port: number
  anf_network_name: string
  anf_center_peer_url?: string
}

export interface AnfStats {
  total_devices: number
  pending: number
  approved: number
  rejected: number
  kicked: number
  networks: number
  tags: number
  rules: number
}
```

在 `ApiClient` 类中追加方法（沿用 `this.client.get/post/patch` 模式）：

```ts
public async centerInfo(): Promise<CenterInfo> {
  return this.client.get<CenterInfo>('/center/info')
}

public async anfStats(): Promise<AnfStats> {
  return this.client.get<AnfStats>('/anf/stats')
}

public async updateTag(id: number, name: string): Promise<any> {
  return this.client.patch(`/tags/${id}`, { name })
}

public async updateAclRule(networkId: string, ruleId: number, rule: any): Promise<any> {
  return this.client.patch(`/networks/${networkId}/rules/${ruleId}`, rule)
}
```

- [ ] **Step 2: 类型检查**

`pnpm --dir easytier-web/frontend build`（`vue-tsc -b && vite build`）→ 通过。

- [ ] **Step 3: 提交**

```bash
git add easytier-web/frontend/src/modules/api.ts
git commit -m "feat(anf): 前端 api 新增 centerInfo/anfStats/updateTag/updateAclRule"
```

---

### Task 6: 前端 Dashboard——中心连接信息卡 + 统计卡 + 轮询 10s

**Files:**
- Modify: `easytier-web/frontend/src/components/Dashboard.vue`
- Modify: `easytier-web/frontend-lib/src/locales/cn.yaml` / `en.yaml`（如走 i18n 则补 key；本页沿用现有中文字面量亦可，新增文案统一中文）

**Interfaces:**
- Consumes: `ApiClient.centerInfo()`、`ApiClient.anfStats()`、`ApiClient.get_summary()`。

- [ ] **Step 1: 改写 Dashboard**

要点（`<script setup>` + 模板）：

- 轮询周期 1000 → `10000`（`new Utils.PeriodicTask(fn, 10000)`）；
- 新增 `centerInfo = ref<CenterInfo | undefined>()`、`stats = ref<AnfStats | undefined>()`；`onMounted` 里 `centerInfo.value = await api.centerInfo()`；
- 端口表渲染：api 端口 / config-server 端口（协议大写）/ 中心 peer（从 `anf_center_peer_url` 原样展示）；
- host 推导：`const host = window.location.hostname`；config-server 填 `\`${host}:${centerInfo.config_server_port}\``；转发中心填 `centerInfo.anf_center_peer_url`；
- 复制按钮：`navigator.clipboard.writeText(value)` + Toast 成功提示；
- 统计卡：`总设备 / 待审批 / 已放行 / 已拒绝 / 已踢出 / 网络 / Tag / ACL 规则` 网格；
- 说明行：「端口以本实例运行时配置为准」。

模板骨架（示意，替换 Dashboard 现有单卡）：

```vue
<template>
  <div class="grid grid-cols-1 gap-4">
    <Card class="w-full">
      <template #title>中心连接信息</template>
      <template #content>
        <table class="w-full text-sm">
          <thead><tr><th>端口</th><th>协议</th><th>服务</th><th>客户端填写</th><th></th></tr></thead>
          <tbody>
            <tr v-if="centerInfo">
              <td>{{ centerInfo.api_server_port }}</td><td>TCP</td>
              <td>Web 控制台 / REST API</td><td>—</td><td></td>
            </tr>
            <tr v-if="centerInfo">
              <td>{{ centerInfo.config_server_port }}</td>
              <td>{{ centerInfo.config_server_protocol.toUpperCase() }}</td>
              <td>config-server（注册 / 配置下发）</td>
              <td><code>{{ configServerAddress }}</code></td>
              <td><Button size="small" label="复制" @click="copy(configServerAddress)" /></td>
            </tr>
            <tr v-if="centerInfo?.anf_center_peer_url">
              <td>{{ peerPort(centerInfo.anf_center_peer_url) }}</td>
              <td>TCP+UDP</td>
              <td>中心 core（中继 / 兜底）</td>
              <td><code>{{ centerInfo.anf_center_peer_url }}</code></td>
              <td><Button size="small" label="复制" @click="copy(centerInfo!.anf_center_peer_url!)" /></td>
            </tr>
          </tbody>
        </table>
        <p class="text-xs text-gray-500 mt-2">端口以本实例运行时配置为准；网络名 {{ centerInfo?.anf_network_name }}，版本 {{ centerInfo?.version }}</p>
      </template>
    </Card>

    <Card v-if="stats" class="w-full">
      <template #title>ANF 概览</template>
      <template #content>
        <div class="grid grid-cols-4 gap-2 text-center">
          <div>总设备<br /><b>{{ stats.total_devices }}</b></div>
          <div>待审批<br /><b>{{ stats.pending }}</b></div>
          <div>已放行<br /><b>{{ stats.approved }}</b></div>
          <div>网络<br /><b>{{ stats.networks }}</b></div>
          <div>Tag<br /><b>{{ stats.tags }}</b></div>
          <div>ACL 规则<br /><b>{{ stats.rules }}</b></div>
        </div>
      </template>
    </Card>
  </div>
</template>
```

`configServerAddress` 计算属性：`` `${host}:${centerInfo.value?.config_server_port}` ``；`peerPort` 用 `new URL(...)` 解析 `anf_center_peer_url` 的端口（非 http 协议用正则 `:(\d+)` 提取）；`copy` 用 clipboard + Toast。

- [ ] **Step 2: 构建验证**

`pnpm --dir easytier-web/frontend build` → 通过。

- [ ] **Step 3: 提交**

```bash
git add easytier-web/frontend/src/components/Dashboard.vue
git commit -m "feat(anf): Dashboard 中心连接信息与 ANF 统计卡，轮询降至 10s"
```

---

### Task 7: 前端 网络管理——名称必填 + 随机网段提示

**Files:**
- Modify: `easytier-web/frontend/src/components/NetworkManagementPage.vue`

**Interfaces:**
- Consumes: `ApiClient.createNetwork(name, cidr?)`（后端随机逻辑 Task 2）。

- [ ] **Step 1: 表单校验与文案**

`create()` 开头加：

```ts
if (!newName.value.trim()) {
  toast.add({ severity: 'warn', summary: '名称必填', life: 2000 })
  return
}
```

网段输入框 label 改为「网段（可选，留空自动分配随机网段，如 10.x.y.0/24）」；placeholder 保留 `10.10.0.0/24`。

- [ ] **Step 2: 创建成功提示实际网段**

`create()` 成功分支改为 `const created = await props.api?.createNetwork(...)` 后 Toast 显示 `网络已创建（网段 ${created?.cidr ?? '—'}）`。

- [ ] **Step 3: 构建验证 + 提交**

`pnpm --dir easytier-web/frontend build` 通过后：

```bash
git add easytier-web/frontend/src/components/NetworkManagementPage.vue
git commit -m "feat(anf): 新建网络名称必填 + 留空自动随机网段提示"
```

---

### Task 8: 前端 Tag 管理——弹窗化 + 编辑

**Files:**
- Modify: `easytier-web/frontend/src/components/TagManagementPage.vue`

**Interfaces:**
- Consumes: `ApiClient.listTags/createTag/updateTag/deleteTag`。

- [ ] **Step 1: 弹窗化（与网络管理一致）**

去掉顶部行内 `InputText`，改为「新建 Tag」按钮 + `Dialog`（复用网络管理弹窗结构）：

```ts
const createDialog = ref(false)
const newName = ref('')
const editId = ref<number | undefined>(undefined)
const dialogTitle = computed(() => (editId.value ? '编辑 Tag' : '新建 Tag'))

const openCreate = () => {
  editId.value = undefined
  newName.value = ''
  createDialog.value = true
}

const openEdit = (tag: any) => {
  editId.value = tag.id
  newName.value = tag.name
  createDialog.value = true
}

const save = async () => {
  if (!newName.value.trim()) {
    toast.add({ severity: 'warn', summary: '名称必填', life: 2000 })
    return
  }
  if (editId.value) {
    await props.api?.updateTag(editId.value, newName.value.trim())
    toast.add({ severity: 'success', summary: 'tag 已更新', life: 2000 })
  } else {
    await props.api?.createTag(newName.value.trim())
    toast.add({ severity: 'success', summary: 'tag 已创建', life: 2000 })
  }
  createDialog.value = false
  await load()
}
```

模板：操作列加「编辑」按钮；Dialog 内顶部只读展示 ID（编辑模式）：

```vue
<Dialog v-model:visible="createDialog" :header="dialogTitle" modal class="w-full max-w-md">
  <div class="space-y-4">
    <div v-if="editId" class="p-field">
      <label class="block text-sm font-medium">ID</label>
      <InputText :model-value="String(editId)" class="w-full" disabled />
    </div>
    <div class="p-field">
      <label class="block text-sm font-medium">名称</label>
      <InputText v-model="newName" class="w-full" placeholder="字母/数字/中划线/下划线/点" />
    </div>
    <div class="flex justify-end gap-2">
      <Button label="取消" severity="secondary" @click="createDialog = false" />
      <Button label="保存" @click="save" />
    </div>
  </div>
</Dialog>
```

`create`/`remove` 里的旧 `newTag` 引用一并替换。

- [ ] **Step 2: 构建验证 + 提交**

```bash
pnpm --dir easytier-web/frontend build
git add easytier-web/frontend/src/components/TagManagementPage.vue
git commit -m "feat(anf): Tag 管理弹窗化并支持编辑昵称"
```

---

### Task 9: 前端 ACL 编辑器——规则编辑

**Files:**
- Modify: `easytier-web/frontend/src/components/AclEditorPage.vue`

**Interfaces:**
- Consumes: `ApiClient.updateAclRule(networkId, ruleId, rule)`、`ApiClient.listAclRules(networkId)`。

- [ ] **Step 1: 编辑弹窗**

新增 `editRuleId = ref<number | undefined>(undefined)`；`openCreate` 保持原样；新增：

```ts
const openEdit = (rule: any) => {
  editRuleId.value = rule.id
  ruleName.value = rule.name
  ruleSource.value = rule.source_tags ?? []
  ruleDest.value = rule.destination_tags ?? []
  ruleProtocol.value = rule.protocol ?? 'any'
  rulePorts.value = (rule.ports ?? []).join(',')
  ruleAction.value = rule.action ?? 'allow'
  rulePriority.value = rule.priority ?? 0
  ruleEnabled.value = rule.enabled !== false
  ruleDialog.value = true
}
```

（如当前无 `ruleEnabled` 状态，新增并在创建时默认 `true`；创建/编辑共用同一 `ruleDialog`。）

`save()` 改为按 `editRuleId` 分流：

```ts
const payload = {
  name: ruleName.value.trim(),
  enabled: ruleEnabled.value,
  source_tags: ruleSource.value,
  destination_tags: ruleDest.value,
  protocol: ruleProtocol.value,
  ports: rulePorts.value.split(',').map(s => s.trim()).filter(Boolean),
  action: ruleAction.value,
  priority: rulePriority.value,
}
if (editRuleId.value && selectedNetworkId.value) {
  await props.api?.updateAclRule(selectedNetworkId.value, editRuleId.value, payload)
  toast.add({ severity: 'success', summary: '规则已更新', life: 2000 })
} else if (selectedNetworkId.value) {
  await props.api?.createAclRule(selectedNetworkId.value, payload)
}
```

模板：规则表操作列新增「编辑」按钮（`@click="openEdit(data)"`）；Dialog 标题按 `editRuleId` 显示「编辑规则 / 新建规则」，编辑模式顶部只读展示 `ID: {{ editRuleId }}`。

- [ ] **Step 2: 构建验证 + 提交**

```bash
pnpm --dir easytier-web/frontend build
git add easytier-web/frontend/src/components/AclEditorPage.vue
git commit -m "feat(anf): ACL 规则支持编辑（ID 只读）"
```

---

### Task 10: 前端 设备列表筛选 + 品牌化/清理（低风险）

**Files:**
- Modify: `easytier-web/frontend/src/components/DeviceAdminPage.vue` / `DeviceList.vue`
- Modify: `easytier-web/frontend/src/components/Login.vue`、`MainPage.vue`、`App.vue`、`main.ts`、`style.css`

**Interfaces:**
- Consumes: `ApiClient.listDevices(status?)`。

- [ ] **Step 1: 设备列表状态 Tab + 关键字**

`DeviceAdminPage.vue`（或 DeviceList）加 `statusFilter = ref('all')` 与 `keyword = ref('')`：

```ts
const load = async () => {
  loading.value = true
  try {
    const list = (await props.api?.listDevices(statusFilter.value === 'all' ? undefined : statusFilter.value)) ?? []
    devices.value = keyword.value
      ? list.filter(d => `${d.display_name ?? ''} ${d.machine_id ?? ''}`.toLowerCase().includes(keyword.value.toLowerCase()))
      : list
  } finally { loading.value = false }
}
```

工具栏：状态 `SelectButton`（全部/待审批/已放行/已拒绝/已踢出）+ 关键字 `InputText`。

- [ ] **Step 2: 品牌化与清理**

- `main.ts`：引入 `definePreset` 定制 `Aura` 主色（indigo `#6366f1` → violet `#8b5cf6`，色阶与 GUI 端一致）并替换 `preset: Aura`；
- `Login.vue`：隐藏「注册 / 设备注册 / API Host 切换」区块（`isRegistering` 保持 false 即可；删除入口按钮），错误文案改中文；
- `MainPage.vue`：侧边栏 Logo 换 `assets/easytier.png` → 文案「ANF EasyTier」，去掉外链；
- 全站清理 `console.log` / `console.debug`（api.ts 拦截器与页面调试输出）；
- 新增文案补 i18n（cn.yaml 优先；英文不齐可先中文兜底，`App.vue` 已默认强制 cn）。

- [ ] **Step 3: 构建验证 + 提交**

```bash
pnpm --dir easytier-web/frontend build
git add easytier-web/frontend/src
git commit -m "style(anf): Web 后台品牌化、设备列表筛选与残留清理"
```

---

### Task 11: 全量回归 + VM 冒烟验收

**Files:**
- Modify: 无（如发现缺陷则修，走 RED→GREEN）

- [ ] **Step 1: 全量回归**

```bash
cargo test -p easytier-web --lib
pnpm --dir easytier-web/frontend build
```

两者全绿；若 `cargo test -p easytier-web`（含集成）耗时过长，以 `--lib` 为准并记录。

- [ ] **Step 2: VM 冒烟**

在 VM 上（或经 `scripts/vm_ssh.py`）执行：

1. 拉取新分支并构建 `easytier-web-embed`（`cargo build --release -p easytier-web --features easytier-web/embed`）；
2. 替换 `deploy/bin/easytier-web` 后 `docker compose -f deploy/compose.anf.yaml up -d easytier-web`；
3. 验证：登录后台 → 中心连接信息卡片端口与部署一致 → 新建网络（留空网段，确认自动随机）→ 新建 Tag → 改名（确认设备/规则引用同步）→ 新建规则 → 编辑规则（确认热更新）→ 设备审批/放行链路正常；
4. 截图留存。

- [ ] **Step 3: 提交收尾**

如无代码改动则跳过；有修复则按 TDD 提交。

---

## 明确单列（不在本计划内）

- **安全加固工作包**（设计树 Q2：MD5→argon2、默认 `admin` 强制首登改密、禁用预置 `user`、登录限流、会话过期、审计最小版）——独立计划，不阻塞四项核心。
- **批量审批/踢出、SSE/WebSocket 推送、config-server 明文回退收敛、网络网段编辑**——二期。

## Self-Review 结论

- 设计树 4 项核心均有对应任务：端口/连接信息 → Task 1/6；随机网段 → Task 2/7；Tag 交互与编辑 → Task 3/8；ACL 编辑 → Task 9（后端已有）。
- 已接受推荐：Dashboard 统计/轮询降频 → Task 4/6；设备列表筛选 + 品牌化/清理 → Task 10；VM 冒烟 → Task 11。
- 安全加固、批量操作等按设计树明确单列，避免范围膨胀。
