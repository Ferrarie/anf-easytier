//! ANFAGENT-30 配置自动下发：托管网络配置生成与 reconcile。
//!
//! 设计规格见 docs/anfagent-30/05-config-distribution-design.md。
//! TDD 用例见本文件底部 `tests` 模块。

use easytier::proto::{
    acl::Acl,
    api::manage::{NetworkingMethod, NetworkConfig},
};
use sea_orm::DbErr;
use uuid::Uuid;

use crate::{
    client_manager::ClientManager,
    db::{Db, UserIdInDb, anf::DeviceStatus},
    webhook::ManagedNetworkConfig,
};

pub const REVISION_PREFIX: &str = "anf-v1-";
const DEFAULT_NETWORK_CIDR: &str = "10.144.0.0/24";

/// ANF 统一虚拟网卡名（便于识别与排查，避免与默认 EasyTier 网卡混淆）。
pub const ANF_TUN_DEV_NAME: &str = "anf_et";

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

/// (网络实例, 设备) → 托管配置 instance_id（确定性名字 UUID，重启不变；每设备唯一）。
/// 用 md5 生成 v3 风格 UUID，避免引入 uuid v5/sha1 新依赖。
fn instance_id_for_device_network(network_id: &str, machine_id: &Uuid) -> Uuid {
    let digest = md5::compute(format!(
        "anf://network/{network_id}/device/{machine_id}"
    ));
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // version 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
    Uuid::from_bytes(bytes)
}

/// 解析 CIDR，返回 (首可用 IP u32, 末可用 IP u32)，跳过网络地址与广播地址。
fn cidr_host_range(cidr: &str) -> Option<(u32, u32)> {
    let (ip_part, prefix_part) = cidr.trim().split_once('/')?;
    let ip: std::net::Ipv4Addr = ip_part.trim().parse().ok()?;
    let prefix: u32 = prefix_part.trim().parse().ok()?;
    if prefix > 30 {
        return None;
    }
    let host_bits = 32 - prefix;
    let network = u32::from(ip) & (u32::MAX << host_bits);
    let broadcast = network | ((1u32 << host_bits) - 1);
    if network + 1 >= broadcast {
        return None;
    }
    Some((network + 1, broadcast - 1))
}

fn ipv4_from_u32(value: u32) -> String {
    std::net::Ipv4Addr::from(value).to_string()
}

impl Db {
    /// 为 approved 设备生成全部托管配置（每分配网络实例一份，含 ACL 编译结果）。
    /// 设备未放行或无网络分配时返回空配置集（不报错），并返回本次 revision。
    pub async fn generate_device_managed_configs(
        &self,
        machine_id: &Uuid,
        template: &AnfConfigTemplate,
    ) -> Result<(Vec<ManagedNetworkConfig>, String), AnfConfigError> {
        let Some(device) = self.get_device_by_machine_id(*machine_id).await? else {
            return Err(AnfConfigError::DeviceNotFound(*machine_id));
        };
        let status = DeviceStatus::from_str(&device.status).unwrap_or(DeviceStatus::Pending);
        if status != DeviceStatus::Approved {
            return Ok((Vec::new(), new_revision()));
        }

        let device_id = device.id;
        let device_tags = self.list_device_tags(device_id).await?;
        let network_ids = self.list_device_networks(device_id).await?;
        self.allocate_device_virtual_ips(device_id, &network_ids).await?;

        let mut configs = Vec::with_capacity(network_ids.len());
        for network_id in &network_ids {
            let acl = self.compile_network_acl(network_id, &device_tags).await?;
            let cidr = self
                .get_network(network_id)
                .await?
                .and_then(|n| n.cidr)
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_NETWORK_CIDR.to_string());
            let network_length = cidr
                .rsplit_once('/')
                .and_then(|(_, p)| p.trim().parse::<i32>().ok())
                .unwrap_or(24);
            let instance_id = instance_id_for_device_network(network_id, machine_id);
            let config = NetworkConfig {
                instance_id: Some(instance_id.to_string()),
                network_name: Some(template.network_name.clone()),
                network_secret: template.network_secret.clone(),
                virtual_ipv4: self.get_device_network_ip(device_id, network_id).await?,
                network_length: Some(network_length),
                networking_method: Some(NetworkingMethod::Manual as i32),
                peer_urls: template.center_peer_url.iter().cloned().collect(),
                hostname: Some(device.display_name.clone()),
                dev_name: Some(ANF_TUN_DEV_NAME.to_string()),
                // 多网卡/走 mesh 场景下让客户端按系统路由表选源接口，
                // 避免 easytier 默认 bind_device=true 绑到“外部”网卡导致连不上中心
                bind_device: Some(false),
                acl: Some(Acl {
                    acl_v1: Some(acl),
                }),
                ..Default::default()
            };
            configs.push(ManagedNetworkConfig {
                instance_id: instance_id.to_string(),
                network_config: serde_json::to_value(&config)?,
            });
        }

        Ok((configs, new_revision()))
    }

    /// 为设备在每个分配网络中分配未使用的虚拟 IP（已分配则复用），并持久化。
    pub async fn allocate_device_virtual_ips(
        &self,
        device_id: i32,
        network_ids: &[String],
    ) -> Result<(), AnfConfigError> {
        for network_id in network_ids {
            if self.get_device_network_ip(device_id, network_id).await?.is_some() {
                continue;
            }
            let cidr = self
                .get_network(network_id)
                .await?
                .and_then(|n| n.cidr)
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_NETWORK_CIDR.to_string());
            let Some((start, end)) = cidr_host_range(&cidr) else {
                continue;
            };
            let used: std::collections::HashSet<u32> = self
                .list_network_used_virtual_ips(network_id)
                .await?
                .iter()
                .filter_map(|ip| ip.parse::<std::net::Ipv4Addr>().ok().map(u32::from))
                .collect();
            let Some(ip) = (start..=end).find(|candidate| !used.contains(candidate)) else {
                continue;
            };
            self.set_device_network_ip(device_id, network_id, &ipv4_from_u32(ip))
                .await?;
        }
        Ok(())
    }

    /// 解析中心用户（设备统一使用的 config server token 对应的用户名）。
    pub async fn resolve_center_user(
        &self,
        center_user_name: &str,
    ) -> Result<UserIdInDb, AnfConfigError> {
        self.get_user_id(center_user_name)
            .await?
            .ok_or_else(|| AnfConfigError::CenterUserNotFound(center_user_name.to_string()))
    }

    /// 某网络下全部 approved 设备的 machine_id。
    pub async fn list_network_approved_machine_ids(
        &self,
        network_id: &str,
    ) -> Result<Vec<Uuid>, DbErr> {
        let devices = self.list_network_devices(network_id).await?;
        Ok(devices
            .iter()
            .filter_map(|d| Uuid::parse_str(&d.machine_id).ok())
            .collect())
    }
}

/// 为设备重新生成并下发全部托管配置（含新 revision）。
pub async fn reconcile_device_configs(
    client_mgr: &ClientManager,
    db: &Db,
    center_user_name: &str,
    machine_id: Uuid,
    template: &AnfConfigTemplate,
) -> anyhow::Result<()> {
    let center_user_id = db.resolve_center_user(center_user_name).await?;
    let (configs, revision) = db
        .generate_device_managed_configs(&machine_id, template)
        .await?;
    client_mgr
        .reconcile_managed_network_configs(
            center_user_id,
            machine_id,
            configs,
            Some(revision),
            None,
        )
        .await
}

/// 撤销设备的全部托管配置（reject/kick 时），并断开在线会话。
pub async fn revoke_device_configs(
    client_mgr: &ClientManager,
    db: &Db,
    center_user_name: &str,
    machine_id: Uuid,
) -> anyhow::Result<()> {
    let center_user_id = db.resolve_center_user(center_user_name).await?;
    client_mgr
        .reconcile_managed_network_configs(
            center_user_id,
            machine_id,
            Vec::new(),
            Some(new_revision()),
            None,
        )
        .await?;
    client_mgr
        .disconnect_session_by_machine_id(center_user_id, &machine_id)
        .await;
    Ok(())
}

/// 网络内全部 approved 设备重新生成配置（ACL 变更热更新）。
pub async fn reconcile_network_device_configs(
    client_mgr: &ClientManager,
    db: &Db,
    center_user_name: &str,
    network_id: &str,
    template: &AnfConfigTemplate,
) -> anyhow::Result<()> {
    let machine_ids = db.list_network_approved_machine_ids(network_id).await?;
    for machine_id in machine_ids {
        reconcile_device_configs(client_mgr, db, center_user_name, machine_id, template).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::anf::DeviceStatus;

    fn template() -> AnfConfigTemplate {
        AnfConfigTemplate::new(
            "anf-m3",
            Some("test-secret".to_string()),
            Some("tcp://127.0.0.1:11110".to_string()),
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
                vec!["tcp://127.0.0.1:11110".to_string()]
            );
            assert_eq!(parsed.hostname.as_deref(), Some(&machine_id.to_string()[..8]));
            assert!(parsed.virtual_ipv4.is_some(), "应分配虚拟 IP");
            assert_eq!(parsed.network_length, Some(24));
        }
    }

    #[tokio::test]
    async fn managed_config_uses_os_routing_bind_device_false() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("办公网", None).await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        register_approved_device(&db, admin, machine_id, &["办公"], &[&net.id]).await;

        let (configs, _) = db
            .generate_device_managed_configs(&machine_id, &template())
            .await
            .unwrap();
        let cfg: NetworkConfig = serde_json::from_value(configs[0].network_config.clone()).unwrap();
        // 多网卡/走 mesh 环境：应让客户端按系统路由表选择源接口，
        // 而不是 easytier 默认绑“外部”网卡（实测会绑到 WLAN 导致连不上中心）。
        assert_eq!(cfg.bind_device, Some(false));
    }

    #[tokio::test]
    async fn managed_config_uses_unified_anf_tun_device_name() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db.create_network("办公网", None).await.unwrap();
        let machine_id = uuid::Uuid::new_v4();
        register_approved_device(&db, admin, machine_id, &["办公"], &[&net.id]).await;

        let (configs, _) = db
            .generate_device_managed_configs(&machine_id, &template())
            .await
            .unwrap();
        let cfg: NetworkConfig = serde_json::from_value(configs[0].network_config.clone()).unwrap();
        // 统一虚拟网卡名，便于识别与排查（避免默认 EasyTier 与既有网卡混淆）
        assert_eq!(cfg.dev_name.as_deref(), Some(ANF_TUN_DEV_NAME));
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

    #[tokio::test]
    async fn resolve_center_user_returns_id_or_clear_error() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        assert_eq!(db.resolve_center_user("cfg-admin").await.unwrap(), admin);
        let err = db.resolve_center_user("missing-user").await.unwrap_err();
        assert!(matches!(err, AnfConfigError::CenterUserNotFound(_)));
    }

    #[tokio::test]
    async fn virtual_ips_are_allocated_within_cidr_distinct_and_stable() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let net = db
            .create_network("办公网", Some("10.99.0.0/24".to_string()))
            .await
            .unwrap();
        let m1 = uuid::Uuid::new_v4();
        let m2 = uuid::Uuid::new_v4();
        register_approved_device(&db, admin, m1, &["办公"], &[&net.id]).await;
        register_approved_device(&db, admin, m2, &["办公"], &[&net.id]).await;

        let (c1, _) = db
            .generate_device_managed_configs(&m1, &template())
            .await
            .unwrap();
        let (c2, _) = db
            .generate_device_managed_configs(&m2, &template())
            .await
            .unwrap();
        let cfg1: NetworkConfig = serde_json::from_value(c1[0].network_config.clone()).unwrap();
        let cfg2: NetworkConfig = serde_json::from_value(c2[0].network_config.clone()).unwrap();
        let ip1 = cfg1.virtual_ipv4.unwrap();
        let ip2 = cfg2.virtual_ipv4.unwrap();
        assert_ne!(ip1, ip2);
        assert_ne!(c1[0].instance_id, c2[0].instance_id, "同网络不同设备 instance_id 必须唯一");
        assert!(ip1.starts_with("10.99.0."));
        assert!(ip2.starts_with("10.99.0."));
        assert_eq!(cfg1.network_length, Some(24));

        let (c1b, _) = db
            .generate_device_managed_configs(&m1, &template())
            .await
            .unwrap();
        let cfg1b: NetworkConfig = serde_json::from_value(c1b[0].network_config.clone()).unwrap();
        assert_eq!(cfg1b.virtual_ipv4.as_deref(), Some(ip1.as_str()));
    }
}
