//! ANFAGENT-30 配置自动下发：托管网络配置生成与 reconcile。
//!
//! 设计规格见 docs/anfagent-30/05-config-distribution-design.md。
//! TDD 用例见本文件底部 `tests` 模块。

use easytier::proto::{
    api::manage::{NetworkingMethod, NetworkConfig},
};
use sea_orm::DbErr;
use uuid::Uuid;

use crate::{
    db::Db,
    webhook::ManagedNetworkConfig,
};

pub const REVISION_PREFIX: &str = "anf-v1-";

/// 托管配置模板：中心 mesh 的网络名/密钥/中心 peer。
#[derive(Debug, Clone)]
pub struct AnfConfigTemplate {
    pub network_name: String,
    pub network_secret: Option<String>,
    pub center_peer_url: Option<String>,
}

impl AnfConfigTemplate {
    pub fn new(
        network_name: &str,
        network_secret: Option<String>,
        center_peer_url: Option<String>,
    ) -> Self {
        Self {
            network_name: network_name.to_string(),
            network_secret,
            center_peer_url,
        }
    }
}

/// 生成新的托管配置 revision（时间戳前缀，保证单调且可读）。
pub fn new_revision() -> String {
    format!(
        "{REVISION_PREFIX}{}",
        chrono::Local::now().timestamp_millis()
    )
}

#[derive(Debug, thiserror::Error)]
pub enum AnfConfigError {
    #[error("设备不存在: {0}")]
    DeviceNotFound(Uuid),
    #[error("设备未放行，无法生成配置: {0}")]
    DeviceNotApproved(Uuid),
    #[error("中心用户不存在: {0}")]
    CenterUserNotFound(String),
    #[error("配置序列化失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Db(#[from] DbErr),
}

/// 网络实例 → 托管配置 instance_id（确定性名字 UUID，重启不变）。
/// 用 md5 生成 v3 风格 UUID，避免引入 uuid v5/sha1 新依赖。
fn instance_id_for_network(network_id: &str) -> Uuid {
    let digest = md5::compute(format!("anf://network/{network_id}"));
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // version 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
    Uuid::from_bytes(bytes)
}

impl Db {
    /// 为 approved 设备生成全部托管配置（每分配网络实例一份，含 ACL 编译结果）。
    /// 设备未放行或无网络分配时返回空配置集（不报错），并返回本次 revision。
    pub async fn generate_device_managed_configs(
        &self,
        machine_id: &Uuid,
        template: &AnfConfigTemplate,
    ) -> Result<(Vec<ManagedNetworkConfig>, String), AnfConfigError> {
        todo!("Task 1 RED 桩：实现见后续步骤")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::anf::DeviceStatus;

    fn template() -> AnfConfigTemplate {
        AnfConfigTemplate::new(
            "anf-m3",
            Some("test-secret".to_string()),
            Some("tcp://10.0.0.6:11110".to_string()),
        )
    }

    async fn admin_user(db: &Db) -> i32 {
        db.auto_create_user("cfg-admin").await.unwrap().id
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
    async fn generate_creates_one_config_per_assigned_network_with_template_fields() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net1 = db.create_network("办公网", None).await.unwrap();
        let net2 = db.create_network("服务器网", None).await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        register_approved_device(&db, admin, machine_id, &["办公"], &[&net1.id, &net2.id]).await;

        let (configs, revision) = db
            .generate_device_managed_configs(&machine_id, &template())
            .await
            .unwrap();

        assert_eq!(configs.len(), 2);
        assert!(revision.starts_with(REVISION_PREFIX));
        for cfg in &configs {
            let parsed: NetworkConfig = serde_json::from_value(cfg.network_config.clone()).unwrap();
            assert_eq!(parsed.network_name.as_deref(), Some("anf-m3"));
            assert_eq!(parsed.network_secret.as_deref(), Some("test-secret"));
            assert_eq!(
                parsed.networking_method,
                Some(NetworkingMethod::Manual as i32)
            );
            assert_eq!(
                parsed.peer_urls,
                vec!["tcp://10.0.0.6:11110".to_string()]
            );
            assert_eq!(parsed.hostname.as_deref(), Some(&machine_id.to_string()[..8]));
        }
    }

    #[tokio::test]
    async fn generated_config_embeds_compiled_acl_with_default_deny() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("办公网", None).await.unwrap();
        db.create_tag("办公").await.unwrap();
        db.create_tag("服务器").await.unwrap();
        db.create_acl_rule(&crate::db::anf_networks::NewAclRule {
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
        let machine_id = uuid::Uuid::new_v4();
        register_approved_device(&db, admin, machine_id, &["办公"], &[&net.id]).await;

        let (configs, _) = db
            .generate_device_managed_configs(&machine_id, &template())
            .await
            .unwrap();
        assert_eq!(configs.len(), 1);
        let cfg: NetworkConfig = serde_json::from_value(configs[0].network_config.clone()).unwrap();
        let acl = cfg.acl.unwrap().acl_v1.unwrap();
        assert_eq!(acl.chains[0].default_action, easytier::proto::acl::Action::Drop as i32);
        assert_eq!(acl.chains[0].rules.len(), 1);
        assert_eq!(
            acl.group.as_ref().unwrap().members,
            vec!["办公".to_string()]
        );
    }

    #[tokio::test]
    async fn generate_returns_empty_for_unapproved_or_networkless_device() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("办公网", None).await.unwrap();
        let invite = db.generate_invite(admin, 10, None).await.unwrap();
        let pending_machine = uuid::Uuid::new_v4();
        db.register_device(&invite.code, pending_machine).await.unwrap();

        let (configs, _) = db
            .generate_device_managed_configs(&pending_machine, &template())
            .await
            .unwrap();
        assert!(configs.is_empty());

        let approved_no_net = uuid::Uuid::new_v4();
        register_approved_device(&db, admin, approved_no_net, &[], &[]).await;
        let (configs, _) = db
            .generate_device_managed_configs(&approved_no_net, &template())
            .await
            .unwrap();
        assert!(configs.is_empty());

        let missing = uuid::Uuid::new_v4();
        let err = db
            .generate_device_managed_configs(&missing, &template())
            .await
            .unwrap_err();
        assert!(matches!(err, AnfConfigError::DeviceNotFound(_)));
        let _ = net.id;
    }

    #[tokio::test]
    async fn instance_id_is_stable_and_revision_changes() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("办公网", None).await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        register_approved_device(&db, admin, machine_id, &["办公"], &[&net.id]).await;

        let (first, rev1) = db
            .generate_device_managed_configs(&machine_id, &template())
            .await
            .unwrap();
        let (second, rev2) = db
            .generate_device_managed_configs(&machine_id, &template())
            .await
            .unwrap();
        assert_eq!(first[0].instance_id, second[0].instance_id);
        assert_ne!(rev1, rev2);
        assert!(rev1.starts_with(REVISION_PREFIX));
        assert!(rev2.starts_with(REVISION_PREFIX));
    }

    #[tokio::test]
    async fn acl_rule_change_regenerates_config_and_revision() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("办公网", None).await.unwrap();
        db.create_tag("办公").await.unwrap();
        db.create_tag("服务器").await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        register_approved_device(&db, admin, machine_id, &["办公"], &[&net.id]).await;

        let (before, rev_before) = db
            .generate_device_managed_configs(&machine_id, &template())
            .await
            .unwrap();
        db.create_acl_rule(&crate::db::anf_networks::NewAclRule {
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
        let (after, rev_after) = db
            .generate_device_managed_configs(&machine_id, &template())
            .await
            .unwrap();

        let before_cfg: NetworkConfig =
            serde_json::from_value(before[0].network_config.clone()).unwrap();
        let after_cfg: NetworkConfig =
            serde_json::from_value(after[0].network_config.clone()).unwrap();
        assert_eq!(
            before_cfg
                .acl
                .as_ref()
                .unwrap()
                .acl_v1
                .as_ref()
                .unwrap()
                .chains[0]
                .rules
                .len(),
            0
        );
        assert_eq!(
            after_cfg
                .acl
                .as_ref()
                .unwrap()
                .acl_v1
                .as_ref()
                .unwrap()
                .chains[0]
                .rules
                .len(),
            1
        );
        assert_ne!(rev_before, rev_after);
    }
}
