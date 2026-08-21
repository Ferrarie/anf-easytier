//! ANFAGENT-30 M2：网络实例 / tag / ACL 规则 CRUD 与 ACL v1 编译（默认拒绝）。
//!
//! 设计规格见 docs/anfagent-30/02-m2-design.md。TDD 用例见本文件底部 `tests` 模块。

use std::collections::BTreeSet;

use chrono::{DateTime, FixedOffset};
use easytier::proto::acl::{
    Action, AclV1, Chain, ChainType, GroupIdentity, GroupInfo, Protocol, Rule,
};
use sea_orm::{
    ColumnTrait, DbErr, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter,
    QueryOrder, Set,
};
use uuid::Uuid;

use super::{Db, entity};
use crate::db::anf::DEVICE_STATUS_APPROVED;

const MAX_ACL_PRIORITY: u32 = 65535;
const NET_ID_PREFIX: &str = "net-";

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
    ) -> Result<entity::network_instances::Model, DbErr> {
        use entity::network_instances;

        let id = format!("{NET_ID_PREFIX}{}", &Uuid::new_v4().simple().to_string()[..8]);
        let m = network_instances::ActiveModel {
            id: Set(id.clone()),
            name: Set(name.trim().to_string()),
            cidr: Set(cidr.map(|c| c.trim().to_string()).filter(|c| !c.is_empty())),
            created_at: Set(now()),
            updated_at: Set(now()),
        };
        network_instances::Entity::insert(m).exec(self.orm_db()).await?;
        self.get_network(&id)
            .await?
            .ok_or_else(|| DbErr::Custom("网络实例创建后未找到".to_string()))
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
        use entity::{device_networks, network_instances};

        if self.get_network(id).await?.is_none() {
            return Err(AnfNetError::NetworkNotFound);
        }
        let in_use = device_networks::Entity::find()
            .filter(device_networks::Column::NetworkInstId.eq(id))
            .count(self.orm_db())
            .await?;
        if in_use > 0 {
            return Err(AnfNetError::NetworkInUse);
        }
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

        Ok(devices::Entity::find()
            .inner_join(device_networks::Entity)
            .filter(device_networks::Column::NetworkInstId.eq(network_id))
            .filter(devices::Column::Status.eq(DEVICE_STATUS_APPROVED))
            .all(self.orm_db())
            .await?)
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

        if tags::Entity::find_by_id(id).one(self.orm_db()).await?.is_none() {
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

        if acl_rules::Entity::find_by_id(id).one(self.orm_db()).await?.is_none() {
            return Err(AnfNetError::RuleNotFound);
        }
        acl_rules::Entity::delete_by_id(id).exec(self.orm_db()).await?;
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

        let net = db.create_network("办公网", Some("10.10.0.0/24".to_string())).await.unwrap();
        assert!(net.id.starts_with(NET_ID_PREFIX));
        assert_eq!(db.list_networks().await.unwrap().len(), 1);

        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公"], &[&net.id])
            .await;
        let err = db.delete_network(&net.id).await.unwrap_err();
        assert!(matches!(err, AnfNetError::NetworkInUse));

        let free = db.create_network("空网", None).await.unwrap();
        db.delete_network(&free.id).await.unwrap();
        assert!(db.get_network(&free.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn tag_crud_and_delete_protection() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;

        let tag = db.create_tag("办公").await.unwrap();
        assert_eq!(db.list_tags().await.unwrap().len(), 1);

        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let d = db.register_device(&invite.code, uuid::Uuid::new_v4()).await.unwrap();
        db.update_device(d.id, None, Some(vec!["办公".to_string()]), None)
            .await
            .unwrap();

        let err = db.delete_tag(tag.id).await.unwrap_err();
        assert!(matches!(err, AnfNetError::TagInUse));

        db.update_device(d.id, None, Some(vec![]), None).await.unwrap();
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

        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公", "mac"], &[&net.id])
            .await;
        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["服务器"], &[&net.id])
            .await;

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

        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公", "服务器"], &[&net.id])
            .await;

        let acl = db.compile_network_acl(&net.id, &["办公".to_string()]).await.unwrap();
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

        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["only-other"], &[&other.id])
            .await;
        register_approved_device(&db, admin, uuid::Uuid::new_v4(), &["办公"], &[&net.id])
            .await;

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

        let err = db.update_acl_rule(9999, &new_rule(&net.id)).await.unwrap_err();
        assert!(matches!(err, AnfNetError::RuleNotFound));
    }
}
