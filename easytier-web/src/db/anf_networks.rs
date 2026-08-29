//! ANFAGENT-30 M2：网络实例 / tag / ACL 规则 CRUD 与 ACL v1 编译（默认拒绝）。
//!
//! 设计规格见 docs/anfagent-30/02-m2-design.md。TDD 用例见本文件底部 `tests` 模块。

use std::collections::BTreeSet;

use chrono::{DateTime, FixedOffset};
use easytier::proto::acl::{
    AclV1, Action, Chain, ChainType, GroupIdentity, GroupInfo, Protocol, Rule,
};
use rand::Rng;
use sea_orm::{
    ColumnTrait, DbErr, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use super::{Db, entity};
use crate::db::anf::DEVICE_STATUS_APPROVED;

const MAX_ACL_PRIORITY: u32 = 65535;
const NET_ID_PREFIX: &str = "net-";

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

#[derive(Debug, thiserror::Error)]
pub enum AnfNetError {
    #[error("网络实例不存在")]
    NetworkNotFound,
    #[error("网络实例仍被设备使用，无法删除")]
    NetworkInUse,
    #[error("tag 不存在")]
    TagNotFound,
    #[error("tag 仍被设备使用，无法删除")]
    TagInUse,
    #[error("ACL 规则不存在")]
    RuleNotFound,
    #[error("无效的 ACL 输入: {0}")]
    InvalidInput(String),
    #[error(transparent)]
    Db(#[from] DbErr),
}

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

fn now() -> DateTime<FixedOffset> {
    chrono::Local::now().fixed_offset()
}

fn json_to_vec(s: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
}

fn vec_to_json(v: &[String]) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string())
}

fn parse_protocol(s: &str) -> Option<i32> {
    match s.to_ascii_lowercase().as_str() {
        "tcp" => Some(Protocol::Tcp as i32),
        "udp" => Some(Protocol::Udp as i32),
        "icmp" => Some(Protocol::Icmp as i32),
        "icmpv6" => Some(Protocol::IcmPv6 as i32),
        "any" => Some(Protocol::Any as i32),
        _ => None,
    }
}

fn parse_action(s: &str) -> Option<i32> {
    match s.to_ascii_lowercase().as_str() {
        "allow" => Some(Action::Allow as i32),
        "drop" => Some(Action::Drop as i32),
        _ => None,
    }
}

fn valid_tag_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !name.chars().any(char::is_whitespace)
}

#[derive(Debug, Clone)]
pub struct NewAclRule {
    pub network_inst_id: String,
    pub name: String,
    pub enabled: bool,
    pub source_tags: Vec<String>,
    pub destination_tags: Vec<String>,
    pub protocol: String,
    pub ports: Vec<String>,
    pub action: String,
    pub priority: u32,
}

impl NewAclRule {
    fn validate(&self) -> Result<(), AnfNetError> {
        if self.name.trim().is_empty() {
            return Err(AnfNetError::InvalidInput("规则名不能为空".to_string()));
        }
        if parse_protocol(&self.protocol).is_none() {
            return Err(AnfNetError::InvalidInput(format!(
                "不支持的协议: {}（tcp/udp/icmp/icmpv6/any）",
                self.protocol
            )));
        }
        if parse_action(&self.action).is_none() {
            return Err(AnfNetError::InvalidInput(format!(
                "不支持的规则动作: {}（allow/drop）",
                self.action
            )));
        }
        if self.priority > MAX_ACL_PRIORITY {
            return Err(AnfNetError::InvalidInput(format!(
                "优先级不能超过 {MAX_ACL_PRIORITY}"
            )));
        }
        for tag in self.source_tags.iter().chain(self.destination_tags.iter()) {
            if !valid_tag_name(tag) {
                return Err(AnfNetError::InvalidInput(format!(
                    "tag 名非法（字母/数字/中划线/下划线/点，≤32 字符，不含空白）: {tag}"
                )));
            }
        }
        Ok(())
    }
}

impl Db {
    // ===== 网络实例 =====

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
                Some(
                    random_cidr(&existing, &mut rand::thread_rng()).ok_or_else(|| {
                        AnfNetError::InvalidInput("随机网段生成失败，请重试".to_string())
                    })?,
                )
            }
        };

        let id = format!(
            "{NET_ID_PREFIX}{}",
            &Uuid::new_v4().simple().to_string()[..8]
        );
        let m = network_instances::ActiveModel {
            id: Set(id.clone()),
            name: Set(name.trim().to_string()),
            cidr: Set(cidr),
            created_at: Set(now()),
            updated_at: Set(now()),
        };
        network_instances::Entity::insert(m)
            .exec(self.orm_db())
            .await?;
        self.get_network(&id)
            .await?
            .ok_or_else(|| AnfNetError::InvalidInput("网络实例创建后未找到".to_string()))
    }

    pub async fn get_network(
        &self,
        id: &str,
    ) -> Result<Option<entity::network_instances::Model>, DbErr> {
        entity::network_instances::Entity::find_by_id(id.to_string())
            .one(self.orm_db())
            .await
    }

    pub async fn list_networks(&self) -> Result<Vec<entity::network_instances::Model>, DbErr> {
        entity::network_instances::Entity::find()
            .order_by_asc(entity::network_instances::Column::Id)
            .all(self.orm_db())
            .await
    }

    pub async fn delete_network(&self, id: &str) -> Result<(), AnfNetError> {
        use entity::{device_networks, devices, network_instances};

        if self.get_network(id).await?.is_none() {
            return Err(AnfNetError::NetworkNotFound);
        }
        // 仅"已放行"设备计入占用（与前端"成员数"统计口径一致）；
        // pending/rejected 设备的网络分配不算占用，避免"成员数为0却删不掉"。
        let approved_in_use = device_networks::Entity::find()
            .inner_join(devices::Entity)
            .filter(device_networks::Column::NetworkInstId.eq(id))
            .filter(devices::Column::Status.eq(DEVICE_STATUS_APPROVED))
            .count(self.orm_db())
            .await?;
        if approved_in_use > 0 {
            return Err(AnfNetError::NetworkInUse);
        }
        // 清理该网络下所有（含 pending/rejected）设备引用，避免孤立行残留。
        device_networks::Entity::delete_many()
            .filter(device_networks::Column::NetworkInstId.eq(id))
            .exec(self.orm_db())
            .await?;
        network_instances::Entity::delete_by_id(id.to_string())
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// 某网络下已放行设备列表（M2 用于 group 编译）。
    pub async fn list_network_devices(
        &self,
        network_id: &str,
    ) -> Result<Vec<entity::devices::Model>, DbErr> {
        use entity::{device_networks, devices};

        devices::Entity::find()
            .inner_join(device_networks::Entity)
            .filter(device_networks::Column::NetworkInstId.eq(network_id))
            .filter(devices::Column::Status.eq(DEVICE_STATUS_APPROVED))
            .all(self.orm_db())
            .await
    }

    // ===== tag =====

    pub async fn create_tag(&self, name: &str) -> Result<entity::tags::Model, DbErr> {
        use entity::tags;

        let m = tags::ActiveModel {
            name: Set(name.trim().to_string()),
            created_at: Set(now()),
            ..Default::default()
        };
        let res = tags::Entity::insert(m).exec(self.orm_db()).await?;
        tags::Entity::find_by_id(res.last_insert_id)
            .one(self.orm_db())
            .await?
            .ok_or_else(|| DbErr::Custom("tag 创建后未找到".to_string()))
    }

    pub async fn list_tags(&self) -> Result<Vec<entity::tags::Model>, DbErr> {
        entity::tags::Entity::find()
            .order_by_asc(entity::tags::Column::Name)
            .all(self.orm_db())
            .await
    }

    /// 统计某 tag 被多少设备引用（前端展示用）。
    pub async fn device_tags_usage(&self, tag: &str) -> Result<usize, DbErr> {
        use entity::device_tags;
        Ok(device_tags::Entity::find()
            .filter(device_tags::Column::Tag.eq(tag))
            .count(self.orm_db())
            .await? as usize)
    }

    pub async fn delete_tag(&self, id: i32) -> Result<(), AnfNetError> {
        use entity::{device_tags, tags};

        if tags::Entity::find_by_id(id)
            .one(self.orm_db())
            .await?
            .is_none()
        {
            return Err(AnfNetError::TagNotFound);
        }
        let tag = tags::Entity::find_by_id(id)
            .one(self.orm_db())
            .await?
            .ok_or(AnfNetError::TagNotFound)?;
        let in_use = device_tags::Entity::find()
            .filter(device_tags::Column::Tag.eq(&tag.name))
            .count(self.orm_db())
            .await?;
        if in_use > 0 {
            return Err(AnfNetError::TagInUse);
        }
        tags::Entity::delete_by_id(id).exec(self.orm_db()).await?;
        Ok(())
    }

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
            device_tags::Entity::insert(device_tags::ActiveModel {
                device_id: Set(row.device_id),
                tag: Set(trimmed.clone()),
            })
            .exec(self.orm_db())
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

    /// ANF 管理统计（Dashboard 数据源）。
    pub async fn anf_stats(&self) -> Result<AnfStats, DbErr> {
        use crate::db::anf::DeviceStatus;
        use entity::{acl_rules, devices, network_instances, tags};

        let all = devices::Entity::find().all(self.orm_db()).await?;
        let count = |status: DeviceStatus| {
            all.iter()
                .filter(|d| DeviceStatus::from_str(&d.status) == Some(status))
                .count() as u32
        };
        Ok(AnfStats {
            total_devices: all.len() as u32,
            pending: count(DeviceStatus::Pending),
            approved: count(DeviceStatus::Approved),
            rejected: count(DeviceStatus::Rejected),
            kicked: count(DeviceStatus::Kicked),
            networks: network_instances::Entity::find()
                .count(self.orm_db())
                .await? as u32,
            tags: tags::Entity::find().count(self.orm_db()).await? as u32,
            rules: acl_rules::Entity::find().count(self.orm_db()).await? as u32,
        })
    }

    // ===== ACL 规则 =====

    pub async fn create_acl_rule(
        &self,
        req: &NewAclRule,
    ) -> Result<entity::acl_rules::Model, AnfNetError> {
        use entity::acl_rules;

        req.validate()?;
        if self.get_network(&req.network_inst_id).await?.is_none() {
            return Err(AnfNetError::NetworkNotFound);
        }

        let m = acl_rules::ActiveModel {
            network_inst_id: Set(req.network_inst_id.clone()),
            name: Set(req.name.trim().to_string()),
            enabled: Set(req.enabled),
            source_tags: Set(vec_to_json(&req.source_tags)),
            destination_tags: Set(vec_to_json(&req.destination_tags)),
            protocol: Set(req.protocol.to_ascii_lowercase()),
            ports: Set(vec_to_json(&req.ports)),
            action: Set(req.action.to_ascii_lowercase()),
            priority: Set(req.priority as i32),
            created_at: Set(now()),
            updated_at: Set(now()),
            ..Default::default()
        };
        let res = acl_rules::Entity::insert(m).exec(self.orm_db()).await?;
        acl_rules::Entity::find_by_id(res.last_insert_id)
            .one(self.orm_db())
            .await?
            .ok_or_else(|| DbErr::Custom("ACL 规则创建后未找到".to_string()).into())
    }

    pub async fn list_acl_rules(
        &self,
        network_id: &str,
    ) -> Result<Vec<entity::acl_rules::Model>, DbErr> {
        entity::acl_rules::Entity::find()
            .filter(entity::acl_rules::Column::NetworkInstId.eq(network_id))
            .order_by_desc(entity::acl_rules::Column::Priority)
            .order_by_asc(entity::acl_rules::Column::Id)
            .all(self.orm_db())
            .await
    }

    pub async fn delete_acl_rule(&self, id: i32) -> Result<(), AnfNetError> {
        use entity::acl_rules;

        if acl_rules::Entity::find_by_id(id)
            .one(self.orm_db())
            .await?
            .is_none()
        {
            return Err(AnfNetError::RuleNotFound);
        }
        acl_rules::Entity::delete_by_id(id)
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// 更新 ACL 规则（全量替换规则字段）。
    pub async fn update_acl_rule(
        &self,
        id: i32,
        req: &NewAclRule,
    ) -> Result<entity::acl_rules::Model, AnfNetError> {
        use entity::acl_rules;

        req.validate()?;
        let existing = acl_rules::Entity::find_by_id(id)
            .one(self.orm_db())
            .await?
            .ok_or(AnfNetError::RuleNotFound)?;
        if self.get_network(&req.network_inst_id).await?.is_none() {
            return Err(AnfNetError::NetworkNotFound);
        }

        let mut m = existing.into_active_model();
        m.network_inst_id = Set(req.network_inst_id.clone());
        m.name = Set(req.name.trim().to_string());
        m.enabled = Set(req.enabled);
        m.source_tags = Set(vec_to_json(&req.source_tags));
        m.destination_tags = Set(vec_to_json(&req.destination_tags));
        m.protocol = Set(req.protocol.to_ascii_lowercase());
        m.ports = Set(vec_to_json(&req.ports));
        m.action = Set(req.action.to_ascii_lowercase());
        m.priority = Set(req.priority as i32);
        m.updated_at = Set(now());
        Ok(acl_rules::Entity::update(m).exec(self.orm_db()).await?)
    }

    // ===== ACL v1 编译 =====

    /// 为某个网络实例编译 ACL v1 配置。
    /// - declares：该网络内全部 tag（已放行设备的 tag ∪ 规则引用的 tag）；
    /// - members：`device_tags` 中已声明部分（即本设备的成员组）；
    /// - chains：启用规则按 priority 降序排列，链默认动作 drop（默认拒绝）。
    pub async fn compile_network_acl(
        &self,
        network_id: &str,
        device_tags: &[String],
    ) -> Result<AclV1, DbErr> {
        let rules = self.list_acl_rules(network_id).await?;
        let devices = self.list_network_devices(network_id).await?;

        let mut all_tags: BTreeSet<String> = BTreeSet::new();
        for device in &devices {
            for tag in self.list_device_tags(device.id).await? {
                all_tags.insert(tag);
            }
        }
        for rule in &rules {
            for tag in json_to_vec(&rule.source_tags)
                .into_iter()
                .chain(json_to_vec(&rule.destination_tags))
            {
                all_tags.insert(tag);
            }
        }

        let declares: Vec<GroupIdentity> = all_tags
            .iter()
            .map(|name| GroupIdentity {
                group_name: name.clone(),
                group_secret: String::new(),
            })
            .collect();
        let members: Vec<String> = device_tags
            .iter()
            .filter(|t| all_tags.contains(*t))
            .cloned()
            .collect();

        let compiled_rules: Vec<Rule> = rules
            .iter()
            .filter(|r| r.enabled)
            .map(|r| Rule {
                name: r.name.clone(),
                description: String::new(),
                priority: r.priority.max(0) as u32,
                enabled: true,
                protocol: parse_protocol(&r.protocol).unwrap_or(Protocol::Any as i32),
                ports: json_to_vec(&r.ports),
                source_ips: Vec::new(),
                destination_ips: Vec::new(),
                source_ports: Vec::new(),
                action: parse_action(&r.action).unwrap_or(Action::Drop as i32),
                rate_limit: 0,
                burst_limit: 0,
                stateful: false,
                source_groups: json_to_vec(&r.source_tags),
                destination_groups: json_to_vec(&r.destination_tags),
            })
            .collect();

        let chains = vec![Chain {
            name: "anf-acl".to_string(),
            chain_type: ChainType::Outbound as i32,
            description: "ANF 中心化管理 ACL（默认拒绝）".to_string(),
            enabled: true,
            rules: compiled_rules,
            default_action: Action::Drop as i32,
        }];

        Ok(AclV1 {
            chains,
            group: Some(GroupInfo { declares, members }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, anf::DeviceStatus};
    use rand::SeedableRng;

    async fn admin_user(db: &Db) -> i32 {
        db.auto_create_user("m2-admin").await.unwrap().id
    }

    fn new_rule(network_id: &str) -> NewAclRule {
        NewAclRule {
            network_inst_id: network_id.to_string(),
            name: "allow-http".to_string(),
            enabled: true,
            source_tags: vec!["办公".to_string()],
            destination_tags: vec!["服务器".to_string()],
            protocol: "tcp".to_string(),
            ports: vec!["80".to_string(), "443".to_string()],
            action: "allow".to_string(),
            priority: 100,
        }
    }

    async fn register_approved_device(
        db: &Db,
        admin: i32,
        machine_id: uuid::Uuid,
        tags: &[&str],
        networks: &[&str],
    ) -> i32 {
        let invite = db.generate_invite(admin, 10, None).await.unwrap();
        let device = db.register_device(&invite.code, machine_id).await.unwrap();
        db.set_device_status(device.id, DeviceStatus::Approved, admin)
            .await
            .unwrap();
        db.update_device(
            device.id,
            None,
            Some(tags.iter().map(|s| s.to_string()).collect()),
            Some(networks.iter().map(|s| s.to_string()).collect()),
        )
        .await
        .unwrap();
        device.id
    }

    #[tokio::test]
    async fn network_crud_and_delete_protection() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;

        let net = db
            .create_network("办公网", Some("10.10.0.0/24".to_string()))
            .await
            .unwrap();
        assert!(net.id.starts_with(NET_ID_PREFIX));
        assert_eq!(db.list_networks().await.unwrap().len(), 1);

        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公"], &[&net.id]).await;
        let err = db.delete_network(&net.id).await.unwrap_err();
        assert!(matches!(err, AnfNetError::NetworkInUse));

        let free = db.create_network("空网", None).await.unwrap();
        db.delete_network(&free.id).await.unwrap();
        assert!(db.get_network(&free.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn network_with_only_pending_assignment_is_deletable_and_cleans_reference() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;

        let net = db.create_network("pending网", None).await.unwrap();
        // 用邀请码注册一个 pending 设备，并只给它分配该网络（尚未放行）。
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let device = db
            .register_device(&invite.code, uuid::Uuid::new_v4())
            .await
            .unwrap();
        db.update_device(
            device.id,
            None,
            Some(vec!["办公".to_string()]),
            Some(vec![net.id.clone()]),
        )
        .await
        .unwrap();

        // 成员数（仅 approved）为 0，删除不应被 pending 分配阻塞。
        assert_eq!(db.list_network_devices(&net.id).await.unwrap().len(), 0);
        db.delete_network(&net.id).await.unwrap();
        assert!(db.get_network(&net.id).await.unwrap().is_none());
        // 设备对该网络的引用也应被清理，避免孤立行。
        assert!(db.list_device_networks(device.id).await.unwrap().is_empty());
    }

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

        let net2 = db.create_network("随机网2", None).await.unwrap();
        assert_ne!(net2.cidr.as_deref(), Some(cidr.as_str()));

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
        for _ in 0..200 {
            let cidr = random_cidr(&existing, &mut rng).unwrap();
            assert!(cidr.starts_with("10.") && cidr.ends_with(".0/24"));
            assert!(!existing.contains(&cidr), "collision: {cidr}");
            assert_ne!(cidr, "10.144.0.0/24");
        }
    }

    #[tokio::test]
    async fn tag_rename_updates_references_and_rejects_duplicates() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;

        let tag = db.create_tag("办公").await.unwrap();
        let net = db
            .create_network("办公网", Some("10.20.0.0/24".to_string()))
            .await
            .unwrap();
        let device =
            register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公"], &[&net.id]).await;

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
        assert!(!tags.contains(&"办公".to_string()));

        // ACL 规则 JSON 级联
        let rules = db.list_acl_rules(&net.id).await.unwrap();
        let updated = rules.iter().find(|r| r.id == rule.id).expect("rule exists");
        assert_eq!(updated.source_tags, vec_to_json(&["办公区".to_string()]));
        assert_eq!(
            updated.destination_tags,
            vec_to_json(&["服务器".to_string()])
        );

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
        let net = db
            .create_network("网", Some("10.30.0.0/24".to_string()))
            .await
            .unwrap();
        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["网关"], &[&net.id]).await;

        let ids = db.list_network_ids_using_tag("网关").await.unwrap();
        assert!(ids.contains(&net.id));
    }

    #[tokio::test]
    async fn anf_stats_counts_devices_by_status_and_resources() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;

        let net = db
            .create_network("网1", Some("10.40.0.0/24".to_string()))
            .await
            .unwrap();
        db.create_tag("办公").await.unwrap();
        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公"], &[&net.id]).await;

        let stats = db.anf_stats().await.unwrap();
        assert_eq!(stats.networks, 1);
        assert_eq!(stats.tags, 1);
        assert_eq!(stats.approved, 1);
        assert_eq!(stats.total_devices, 1);
        assert_eq!(
            stats.total_devices,
            stats.pending + stats.approved + stats.rejected + stats.kicked
        );
    }

    #[tokio::test]
    async fn tag_crud_and_delete_protection() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;

        let tag = db.create_tag("办公").await.unwrap();
        assert_eq!(db.list_tags().await.unwrap().len(), 1);

        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let d = db
            .register_device(&invite.code, uuid::Uuid::new_v4())
            .await
            .unwrap();
        db.update_device(d.id, None, Some(vec!["办公".to_string()]), None)
            .await
            .unwrap();

        let err = db.delete_tag(tag.id).await.unwrap_err();
        assert!(matches!(err, AnfNetError::TagInUse));

        db.update_device(d.id, None, Some(vec![]), None)
            .await
            .unwrap();
        db.delete_tag(tag.id).await.unwrap();
        assert!(db.list_tags().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn acl_rule_validation() {
        let db = Db::memory_db().await;
        let net = db.create_network("n", None).await.unwrap();

        let mut bad_proto = new_rule(&net.id);
        bad_proto.protocol = "sctp".to_string();
        assert!(matches!(
            db.create_acl_rule(&bad_proto).await.unwrap_err(),
            AnfNetError::InvalidInput(_)
        ));

        let mut bad_action = new_rule(&net.id);
        bad_action.action = "reject".to_string();
        assert!(matches!(
            db.create_acl_rule(&bad_action).await.unwrap_err(),
            AnfNetError::InvalidInput(_)
        ));

        let mut bad_tag = new_rule(&net.id);
        bad_tag.source_tags = vec!["bad tag".to_string()];
        assert!(matches!(
            db.create_acl_rule(&bad_tag).await.unwrap_err(),
            AnfNetError::InvalidInput(_)
        ));

        let rule = db.create_acl_rule(&new_rule(&net.id)).await.unwrap();
        assert_eq!(rule.priority, 100);
    }

    #[tokio::test]
    async fn compile_default_deny_without_rules() {
        let db = Db::memory_db().await;
        let net = db.create_network("n", None).await.unwrap();
        let acl = db.compile_network_acl(&net.id, &[]).await.unwrap();

        assert_eq!(acl.chains.len(), 1);
        assert_eq!(acl.chains[0].default_action, Action::Drop as i32);
        assert!(acl.chains[0].rules.is_empty());
        let group = acl.group.unwrap();
        assert!(group.declares.is_empty());
        assert!(group.members.is_empty());
    }

    #[tokio::test]
    async fn compile_declares_all_tags_and_members_only_device_tags() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("n", None).await.unwrap();

        register_approved_device(
            &db,
            admin,
            uuid::Uuid::new_v4(),
            &["办公", "mac"],
            &[&net.id],
        )
        .await;
        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["服务器"], &[&net.id]).await;

        let acl = db
            .compile_network_acl(&net.id, &["mac".to_string()])
            .await
            .unwrap();
        let group = acl.group.unwrap();
        let declared: Vec<String> = group
            .declares
            .iter()
            .map(|d| d.group_name.clone())
            .collect();
        assert_eq!(declared, vec!["mac", "办公", "服务器"]);
        assert_eq!(group.members, vec!["mac"]);
    }

    #[tokio::test]
    async fn compile_rules_sorted_by_priority_desc_with_fields_mapped() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("n", None).await.unwrap();

        let mut low = new_rule(&net.id);
        low.name = "low".to_string();
        low.priority = 10;
        db.create_acl_rule(&low).await.unwrap();

        let mut high = new_rule(&net.id);
        high.name = "high".to_string();
        high.priority = 200;
        high.protocol = "udp".to_string();
        high.ports = vec!["53".to_string()];
        high.action = "drop".to_string();
        db.create_acl_rule(&high).await.unwrap();

        register_approved_device(
            &db,
            admin,
            uuid::Uuid::new_v4(),
            &["办公", "服务器"],
            &[&net.id],
        )
        .await;

        let acl = db
            .compile_network_acl(&net.id, &["办公".to_string()])
            .await
            .unwrap();
        let rules = &acl.chains[0].rules;
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "high");
        assert_eq!(rules[1].name, "low");
        assert_eq!(rules[0].protocol, Protocol::Udp as i32);
        assert_eq!(rules[0].ports, vec!["53"]);
        assert_eq!(rules[0].action, Action::Drop as i32);
        assert_eq!(rules[0].source_groups, vec!["办公"]);
        assert_eq!(rules[0].destination_groups, vec!["服务器"]);
        assert_eq!(rules[1].action, Action::Allow as i32);
    }

    #[tokio::test]
    async fn compile_ignores_disabled_rules_and_out_of_network_devices() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("n", None).await.unwrap();
        let other = db.create_network("m", None).await.unwrap();

        let mut disabled = new_rule(&net.id);
        disabled.name = "disabled".to_string();
        disabled.enabled = false;
        db.create_acl_rule(&disabled).await.unwrap();

        register_approved_device(
            &db,
            admin,
            uuid::Uuid::new_v4(),
            &["only-other"],
            &[&other.id],
        )
        .await;
        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公"], &[&net.id]).await;

        let acl = db.compile_network_acl(&net.id, &[]).await.unwrap();
        assert!(acl.chains[0].rules.is_empty());
        let declared: Vec<String> = acl
            .group
            .unwrap()
            .declares
            .iter()
            .map(|d| d.group_name.clone())
            .collect();
        assert!(!declared.contains(&"only-other".to_string()));
        assert!(declared.contains(&"办公".to_string()));
    }

    #[tokio::test]
    async fn delete_missing_network_tag_rule_fail() {
        let db = Db::memory_db().await;
        assert!(matches!(
            db.delete_network("net-missing").await.unwrap_err(),
            AnfNetError::NetworkNotFound
        ));
        assert!(matches!(
            db.delete_tag(9999).await.unwrap_err(),
            AnfNetError::TagNotFound
        ));
        assert!(matches!(
            db.delete_acl_rule(9999).await.unwrap_err(),
            AnfNetError::RuleNotFound
        ));
    }

    #[tokio::test]
    async fn update_acl_rule_replaces_fields() {
        let db = Db::memory_db().await;
        let net = db.create_network("n", None).await.unwrap();
        let rule = db.create_acl_rule(&new_rule(&net.id)).await.unwrap();

        let mut updated = new_rule(&net.id);
        updated.name = "renamed".to_string();
        updated.priority = 7;
        updated.action = "drop".to_string();
        let saved = db.update_acl_rule(rule.id, &updated).await.unwrap();
        assert_eq!(saved.name, "renamed");
        assert_eq!(saved.priority, 7);
        assert_eq!(saved.action, "drop");

        let err = db
            .update_acl_rule(9999, &new_rule(&net.id))
            .await
            .unwrap_err();
        assert!(matches!(err, AnfNetError::RuleNotFound));
    }
}
