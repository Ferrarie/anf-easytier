//! ANFAGENT-30 中心化授权核心数据逻辑：邀请码 / 设备审批 / 管理员绑定。
//!
//! 设计规格见 docs/anfagent-30/01-m1-design.md。TDD 用例见本文件底部 `tests` 模块。

use chrono::{DateTime, FixedOffset};
use sea_orm::{
    ColumnTrait, DbErr, EntityTrait, IntoActiveModel, JoinType, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait, Set, TransactionTrait,
};
use uuid::Uuid;

use super::{Db, UserIdInDb, entity};

pub const DEVICE_STATUS_PENDING: &str = "pending";
pub const DEVICE_STATUS_APPROVED: &str = "approved";
pub const DEVICE_STATUS_REJECTED: &str = "rejected";
pub const DEVICE_STATUS_KICKED: &str = "kicked";

const SUPERUSERS_GROUP: &str = "superusers";

/// 用户 TOTP 两步验证状态快照（users 表对应字段）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoFactorState {
    pub secret_encrypted: Option<String>,
    pub enabled: bool,
    pub fail_count: i64,
    pub lock_until: Option<i64>,
    pub last_step: Option<i64>,
}

/// 管理后台用户列表行
#[derive(Debug, Clone, serde::Serialize)]
pub struct AdminUserRow {
    pub id: i32,
    pub username: String,
    pub is_superuser: bool,
    pub totp_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceStatus {
    Pending,
    Approved,
    Rejected,
    Kicked,
}

impl DeviceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => DEVICE_STATUS_PENDING,
            Self::Approved => DEVICE_STATUS_APPROVED,
            Self::Rejected => DEVICE_STATUS_REJECTED,
            Self::Kicked => DEVICE_STATUS_KICKED,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            DEVICE_STATUS_PENDING => Some(Self::Pending),
            DEVICE_STATUS_APPROVED => Some(Self::Approved),
            DEVICE_STATUS_REJECTED => Some(Self::Rejected),
            DEVICE_STATUS_KICKED => Some(Self::Kicked),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnfError {
    #[error("邀请码不存在或已吊销")]
    InviteNotFound,
    #[error("邀请码已过期")]
    InviteExpired,
    #[error("邀请码使用次数已用尽")]
    InviteUsedUp,
    #[error("设备不存在")]
    DeviceNotFound,
    #[error("用户不存在")]
    UserNotFound,
    #[error("非法的设备状态流转: {0} -> {1}")]
    InvalidTransition(String, String),
    #[error(transparent)]
    Db(#[from] DbErr),
}

fn now() -> DateTime<FixedOffset> {
    chrono::Local::now().fixed_offset()
}

fn default_display_name(machine_id: Uuid) -> String {
    machine_id.simple().to_string()[..8].to_string()
}

impl Db {
    /// 生成唯一邀请码（12 位十六进制，来自随机 UUID）。
    pub async fn generate_invite(
        &self,
        created_by: UserIdInDb,
        max_uses: i32,
        expires_at: Option<DateTime<FixedOffset>>,
    ) -> Result<entity::invite_codes::Model, DbErr> {
        use entity::invite_codes;

        let mut code = String::new();
        for _ in 0..8 {
            let candidate = Uuid::new_v4().simple().to_string()[..12].to_string();
            let exists = invite_codes::Entity::find()
                .filter(invite_codes::Column::Code.eq(&candidate))
                .one(self.orm_db())
                .await?
                .is_some();
            if !exists {
                code = candidate;
                break;
            }
        }
        if code.is_empty() {
            return Err(DbErr::Custom("邀请码生成冲突，请重试".to_string()));
        }

        let m = invite_codes::ActiveModel {
            code: Set(code),
            created_by: Set(created_by),
            max_uses: Set(max_uses.max(1)),
            used_count: Set(0),
            expires_at: Set(expires_at),
            enabled: Set(true),
            created_at: Set(now()),
            ..Default::default()
        };
        let res = invite_codes::Entity::insert(m).exec(self.orm_db()).await?;
        invite_codes::Entity::find_by_id(res.last_insert_id)
            .one(self.orm_db())
            .await?
            .ok_or_else(|| DbErr::Custom("邀请码创建后未找到".to_string()))
    }

    pub async fn list_invites(&self) -> Result<Vec<entity::invite_codes::Model>, DbErr> {
        use entity::invite_codes;
        invite_codes::Entity::find()
            .order_by_asc(invite_codes::Column::Id)
            .all(self.orm_db())
            .await
    }

    /// 吊销邀请码（enabled = false）。
    pub async fn disable_invite(&self, id: i32) -> Result<(), DbErr> {
        use entity::invite_codes;
        invite_codes::Entity::update_many()
            .filter(invite_codes::Column::Id.eq(id))
            .col_expr(
                invite_codes::Column::Enabled,
                sea_orm::prelude::Expr::value(false),
            )
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// 设备凭邀请码注册。校验不通过返回对应错误；通过则消耗一次邀请码并登记设备（pending）。
    /// 已放行设备重复注册保持 approved；其余情况重置为 pending。
    pub async fn register_device(
        &self,
        invite_code: &str,
        machine_id: Uuid,
    ) -> Result<entity::devices::Model, AnfError> {
        use entity::{devices, invite_codes};

        let txn = self.orm_db().begin().await?;
        let invite = invite_codes::Entity::find()
            .filter(invite_codes::Column::Code.eq(invite_code))
            .one(&txn)
            .await?
            .ok_or(AnfError::InviteNotFound)?;

        if !invite.enabled {
            return Err(AnfError::InviteNotFound);
        }
        if let Some(expires_at) = invite.expires_at
            && expires_at < now()
        {
            return Err(AnfError::InviteExpired);
        }
        if invite.used_count >= invite.max_uses {
            return Err(AnfError::InviteUsedUp);
        }

        let mut inv_active = invite.clone().into_active_model();
        inv_active.used_count = Set(invite.used_count + 1);
        invite_codes::Entity::update(inv_active).exec(&txn).await?;

        let machine = machine_id.to_string();
        let existing = devices::Entity::find()
            .filter(devices::Column::MachineId.eq(&machine))
            .one(&txn)
            .await?;

        let device = match existing {
            Some(d) if d.status == DEVICE_STATUS_APPROVED => d,
            Some(d) => {
                let mut m = d.into_active_model();
                m.status = Set(DEVICE_STATUS_PENDING.to_string());
                m.approved_by = Set(None);
                m.approved_at = Set(None);
                m.updated_at = Set(now());
                devices::Entity::update(m).exec(&txn).await?
            }
            None => {
                let m = devices::ActiveModel {
                    machine_id: Set(machine.clone()),
                    display_name: Set(default_display_name(machine_id)),
                    status: Set(DEVICE_STATUS_PENDING.to_string()),
                    approved_by: Set(None),
                    approved_at: Set(None),
                    created_at: Set(now()),
                    updated_at: Set(now()),
                    ..Default::default()
                };
                let res = devices::Entity::insert(m).exec(&txn).await?;
                devices::Entity::find_by_id(res.last_insert_id)
                    .one(&txn)
                    .await?
                    .ok_or_else(|| DbErr::Custom("设备创建后未找到".to_string()))?
            }
        };

        txn.commit().await?;
        Ok(device)
    }

    pub async fn get_device(&self, id: i32) -> Result<Option<entity::devices::Model>, DbErr> {
        entity::devices::Entity::find_by_id(id)
            .one(self.orm_db())
            .await
    }

    pub async fn get_device_by_machine_id(
        &self,
        machine_id: Uuid,
    ) -> Result<Option<entity::devices::Model>, DbErr> {
        use entity::devices;
        devices::Entity::find()
            .filter(devices::Column::MachineId.eq(machine_id.to_string()))
            .one(self.orm_db())
            .await
    }

    pub async fn list_devices(
        &self,
        status: Option<DeviceStatus>,
    ) -> Result<Vec<entity::devices::Model>, DbErr> {
        use entity::devices;
        let mut q = devices::Entity::find();
        match status {
            Some(s) => {
                q = q.filter(devices::Column::Status.eq(s.as_str()));
            }
            None => {
                // 默认列表全量显示所有设备（含已拒绝/已踢出），便于管理员审计与二次处理。
                // 参考 Tailscale 授权页样式，能看到全部设备；同一机器码重新注册会回到 pending。
            }
        }
        q.order_by_asc(devices::Column::Id).all(self.orm_db()).await
    }

    /// 删除设备记录（彻底移除不复用），返回受影响行数。
    pub async fn delete_device(&self, id: i32) -> Result<bool, DbErr> {
        use entity::{device_networks, device_tags, devices};
        let res = device_tags::Entity::delete_many()
            .filter(device_tags::Column::DeviceId.eq(id))
            .exec(self.orm_db())
            .await?;
        let _ = res;
        device_networks::Entity::delete_many()
            .filter(device_networks::Column::DeviceId.eq(id))
            .exec(self.orm_db())
            .await?;
        let res = devices::Entity::delete_by_id(id)
            .exec(self.orm_db())
            .await?;
        Ok(res.rows_affected > 0)
    }

    /// 设备状态机：pending → approved / rejected / kicked；approved → rejected / kicked；
    /// rejected / kicked 为终态（重新注册可回到 pending）。
    pub async fn set_device_status(
        &self,
        id: i32,
        new_status: DeviceStatus,
        actor_user_id: UserIdInDb,
    ) -> Result<entity::devices::Model, AnfError> {
        use entity::devices;

        let d = devices::Entity::find_by_id(id)
            .one(self.orm_db())
            .await?
            .ok_or(AnfError::DeviceNotFound)?;
        let current = DeviceStatus::from_str(&d.status).unwrap_or(DeviceStatus::Pending);

        let allowed = matches!(
            (current, new_status),
            (DeviceStatus::Pending, DeviceStatus::Approved)
                | (DeviceStatus::Pending, DeviceStatus::Rejected)
                | (DeviceStatus::Pending, DeviceStatus::Kicked)
                | (DeviceStatus::Approved, DeviceStatus::Rejected)
                | (DeviceStatus::Approved, DeviceStatus::Kicked)
        );
        if !allowed {
            return Err(AnfError::InvalidTransition(
                current.as_str().to_string(),
                new_status.as_str().to_string(),
            ));
        }

        let mut m = d.into_active_model();
        m.status = Set(new_status.as_str().to_string());
        m.approved_by = if new_status == DeviceStatus::Approved {
            Set(Some(actor_user_id))
        } else {
            Set(None)
        };
        m.approved_at = if new_status == DeviceStatus::Approved {
            Set(Some(now()))
        } else {
            Set(None)
        };
        m.updated_at = Set(now());
        Ok(devices::Entity::update(m).exec(self.orm_db()).await?)
    }

    /// 改名 / 全量替换 tag / 全量替换网络分配（事务）。
    pub async fn update_device(
        &self,
        id: i32,
        display_name: Option<String>,
        tags: Option<Vec<String>>,
        networks: Option<Vec<String>>,
    ) -> Result<entity::devices::Model, AnfError> {
        use entity::{device_networks, device_tags, devices};

        let txn = self.orm_db().begin().await?;
        let d = devices::Entity::find_by_id(id)
            .one(&txn)
            .await?
            .ok_or(AnfError::DeviceNotFound)?;

        let mut m = d.into_active_model();
        if let Some(name) = display_name {
            let trimmed = name.trim().to_string();
            if !trimmed.is_empty() {
                m.display_name = Set(trimmed);
            }
        }
        m.updated_at = Set(now());
        devices::Entity::update(m).exec(&txn).await?;

        if let Some(tags) = tags {
            device_tags::Entity::delete_many()
                .filter(device_tags::Column::DeviceId.eq(id))
                .exec(&txn)
                .await?;
            for tag in tags {
                let tag = tag.trim().to_string();
                if tag.is_empty() {
                    continue;
                }
                device_tags::Entity::insert(device_tags::ActiveModel {
                    device_id: Set(id),
                    tag: Set(tag),
                })
                .exec(&txn)
                .await?;
            }
        }

        if let Some(networks) = networks {
            // 保留已有虚拟 IP（同网络重新分配时不变化，保证地址稳定）
            let existing_ips: std::collections::HashMap<String, String> =
                device_networks::Entity::find()
                    .filter(device_networks::Column::DeviceId.eq(id))
                    .all(&txn)
                    .await?
                    .into_iter()
                    .filter_map(|m| m.virtual_ip.map(|ip| (m.network_inst_id, ip)))
                    .collect();
            device_networks::Entity::delete_many()
                .filter(device_networks::Column::DeviceId.eq(id))
                .exec(&txn)
                .await?;
            for network in networks {
                let network = network.trim().to_string();
                if network.is_empty() {
                    continue;
                }
                device_networks::Entity::insert(device_networks::ActiveModel {
                    device_id: Set(id),
                    network_inst_id: Set(network.clone()),
                    virtual_ip: Set(existing_ips.get(&network).cloned()),
                })
                .exec(&txn)
                .await?;
            }
        }

        txn.commit().await?;
        self.get_device(id).await?.ok_or(AnfError::DeviceNotFound)
    }

    pub async fn list_device_tags(&self, device_id: i32) -> Result<Vec<String>, DbErr> {
        use entity::device_tags;
        Ok(device_tags::Entity::find()
            .filter(device_tags::Column::DeviceId.eq(device_id))
            .order_by_asc(device_tags::Column::Tag)
            .all(self.orm_db())
            .await?
            .into_iter()
            .map(|m| m.tag)
            .collect())
    }

    pub async fn list_device_networks(&self, device_id: i32) -> Result<Vec<String>, DbErr> {
        use entity::device_networks;
        Ok(device_networks::Entity::find()
            .filter(device_networks::Column::DeviceId.eq(device_id))
            .order_by_asc(device_networks::Column::NetworkInstId)
            .all(self.orm_db())
            .await?
            .into_iter()
            .map(|m| m.network_inst_id)
            .collect())
    }

    /// 设备在某网络下已分配的虚拟 IP。
    pub async fn get_device_network_ip(
        &self,
        device_id: i32,
        network_id: &str,
    ) -> Result<Option<String>, DbErr> {
        use entity::device_networks;
        Ok(device_networks::Entity::find()
            .filter(device_networks::Column::DeviceId.eq(device_id))
            .filter(device_networks::Column::NetworkInstId.eq(network_id))
            .one(self.orm_db())
            .await?
            .and_then(|m| m.virtual_ip))
    }

    /// 某网络下全部已分配的虚拟 IP。
    pub async fn list_network_used_virtual_ips(
        &self,
        network_id: &str,
    ) -> Result<Vec<String>, DbErr> {
        use entity::device_networks;
        Ok(device_networks::Entity::find()
            .filter(device_networks::Column::NetworkInstId.eq(network_id))
            .all(self.orm_db())
            .await?
            .into_iter()
            .filter_map(|m| m.virtual_ip)
            .collect())
    }

    /// 持久化设备在某网络的虚拟 IP。
    pub async fn set_device_network_ip(
        &self,
        device_id: i32,
        network_id: &str,
        ip: &str,
    ) -> Result<(), DbErr> {
        use entity::device_networks;
        device_networks::Entity::update_many()
            .filter(device_networks::Column::DeviceId.eq(device_id))
            .filter(device_networks::Column::NetworkInstId.eq(network_id))
            .col_expr(
                device_networks::Column::VirtualIp,
                sea_orm::prelude::Expr::value(ip.to_string()),
            )
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// SSH 引导：把机器码绑定为指定用户的管理员设备（幂等）。
    /// 设备记录不存在则创建并直接 approved；用户会被加入 superusers 组。
    pub async fn bind_admin_device(
        &self,
        machine_id: Uuid,
        user_id: UserIdInDb,
    ) -> Result<entity::admin_devices::Model, AnfError> {
        use entity::{admin_devices, devices};

        let txn = self.orm_db().begin().await?;

        let user_exists = entity::users::Entity::find_by_id(user_id)
            .one(&txn)
            .await?
            .is_some();
        if !user_exists {
            return Err(AnfError::UserNotFound);
        }

        let machine = machine_id.to_string();
        let existing = devices::Entity::find()
            .filter(devices::Column::MachineId.eq(&machine))
            .one(&txn)
            .await?;
        match existing {
            Some(d) if d.status != DEVICE_STATUS_APPROVED => {
                let mut m = d.into_active_model();
                m.status = Set(DEVICE_STATUS_APPROVED.to_string());
                m.approved_by = Set(Some(user_id));
                m.approved_at = Set(Some(now()));
                m.updated_at = Set(now());
                devices::Entity::update(m).exec(&txn).await?;
            }
            Some(_) => {}
            None => {
                let m = devices::ActiveModel {
                    machine_id: Set(machine.clone()),
                    display_name: Set(default_display_name(machine_id)),
                    status: Set(DEVICE_STATUS_APPROVED.to_string()),
                    approved_by: Set(Some(user_id)),
                    approved_at: Set(Some(now())),
                    created_at: Set(now()),
                    updated_at: Set(now()),
                    ..Default::default()
                };
                devices::Entity::insert(m).exec(&txn).await?;
            }
        }

        let admin = match admin_devices::Entity::find()
            .filter(admin_devices::Column::MachineId.eq(&machine))
            .one(&txn)
            .await?
        {
            Some(a) if a.user_id != user_id => {
                let mut m = a.into_active_model();
                m.user_id = Set(user_id);
                admin_devices::Entity::update(m).exec(&txn).await?
            }
            Some(a) => a,
            None => {
                let m = admin_devices::ActiveModel {
                    machine_id: Set(machine),
                    user_id: Set(user_id),
                    created_at: Set(now()),
                    ..Default::default()
                };
                let res = admin_devices::Entity::insert(m).exec(&txn).await?;
                admin_devices::Entity::find_by_id(res.last_insert_id)
                    .one(&txn)
                    .await?
                    .ok_or_else(|| DbErr::Custom("管理员设备创建后未找到".to_string()))?
            }
        };

        Self::ensure_user_in_superusers_txn(&txn, user_id).await?;
        txn.commit().await?;
        Ok(admin)
    }

    pub async fn list_admin_devices(&self) -> Result<Vec<entity::admin_devices::Model>, DbErr> {
        use entity::admin_devices;
        admin_devices::Entity::find()
            .order_by_asc(admin_devices::Column::Id)
            .all(self.orm_db())
            .await
    }

    pub async fn user_is_superuser(&self, user_id: UserIdInDb) -> Result<bool, DbErr> {
        use entity::{groups, users_groups};
        let count = users_groups::Entity::find()
            .join(JoinType::InnerJoin, users_groups::Relation::Groups.def())
            .filter(users_groups::Column::UserId.eq(user_id))
            .filter(groups::Column::Name.eq(SUPERUSERS_GROUP))
            .count(self.orm_db())
            .await?;
        Ok(count > 0)
    }

    // ===== ANF TOTP 两步验证（设计共识 2026-08-29）=====

    /// 读取用户 TOTP 2FA 状态
    pub async fn get_2fa_state(&self, user_id: UserIdInDb) -> anyhow::Result<TwoFactorState> {
        use entity::users;
        let u = users::Entity::find_by_id(user_id)
            .one(self.orm_db())
            .await?
            .ok_or_else(|| anyhow::anyhow!("用户不存在: {user_id}"))?;
        Ok(TwoFactorState {
            secret_encrypted: u.totp_secret_encrypted,
            enabled: u.totp_enabled,
            fail_count: u.totp_fail_count as i64,
            lock_until: u.totp_lock_until,
            last_step: u.totp_last_step,
        })
    }

    /// 是否已启用 TOTP 2FA
    pub async fn is_2fa_enabled(&self, user_id: UserIdInDb) -> anyhow::Result<bool> {
        Ok(self.get_2fa_state(user_id).await?.enabled)
    }

    /// 记录一次 2FA 验证失败；每满 FAILS_PER_LOCK 次设置锁定截止时间并返回之
    pub async fn record_2fa_fail(
        &self,
        user_id: UserIdInDb,
        now_ts: i64,
    ) -> anyhow::Result<Option<i64>> {
        use entity::users;
        let u = users::Entity::find_by_id(user_id)
            .one(self.orm_db())
            .await?
            .ok_or_else(|| anyhow::anyhow!("用户不存在: {user_id}"))?;
        let new_count = u.totp_fail_count as i64 + 1;
        let mut new_lock = None;
        let mut updates = users::ActiveModel {
            id: Set(user_id),
            ..Default::default()
        };
        updates.totp_fail_count = Set(new_count as i32);
        if let Some(until) = crate::anf::two_factor::lock_until_after_fail(new_count, now_ts) {
            updates.totp_lock_until = Set(Some(until));
            new_lock = Some(until);
        }
        users::Entity::update(updates).exec(self.orm_db()).await?;
        Ok(new_lock)
    }

    /// 2FA 验证成功后清零失败计数与锁定状态
    pub async fn clear_2fa_fail(&self, user_id: UserIdInDb) -> anyhow::Result<()> {
        use entity::users;
        users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id))
            .col_expr(
                users::Column::TotpFailCount,
                sea_orm::prelude::Expr::value(0),
            )
            .col_expr(
                users::Column::TotpLockUntil,
                sea_orm::prelude::Expr::value(None::<i64>),
            )
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// 写入（覆盖）加密后的 TOTP secret，处于未启用状态（绑定流程第一步）
    pub async fn set_totp_secret(
        &self,
        user_id: UserIdInDb,
        secret_encrypted: String,
    ) -> anyhow::Result<()> {
        use entity::users;
        users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id))
            .col_expr(
                users::Column::TotpSecretEncrypted,
                sea_orm::prelude::Expr::value(secret_encrypted),
            )
            .col_expr(
                users::Column::TotpEnabled,
                sea_orm::prelude::Expr::value(false),
            )
            .col_expr(
                users::Column::TotpLastStep,
                sea_orm::prelude::Expr::value(None::<i64>),
            )
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// 绑定验证通过：启用 TOTP 并记录本次窗口（防重放基线）
    pub async fn enable_totp(&self, user_id: UserIdInDb, last_step: i64) -> anyhow::Result<()> {
        use entity::users;
        users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id))
            .col_expr(
                users::Column::TotpEnabled,
                sea_orm::prelude::Expr::value(true),
            )
            .col_expr(
                users::Column::TotpLastStep,
                sea_orm::prelude::Expr::value(last_step),
            )
            .col_expr(
                users::Column::TotpFailCount,
                sea_orm::prelude::Expr::value(0),
            )
            .col_expr(
                users::Column::TotpLockUntil,
                sea_orm::prelude::Expr::value(None::<i64>),
            )
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// 登录验证成功后更新防重放窗口基线
    pub async fn set_2fa_last_step(&self, user_id: UserIdInDb, step: i64) -> anyhow::Result<()> {
        use entity::users;
        users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id))
            .col_expr(
                users::Column::TotpLastStep,
                sea_orm::prelude::Expr::value(step),
            )
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// 完全重置用户 TOTP 2FA（管理员后台 / CLI 救援）
    pub async fn clear_totp(&self, user_id: UserIdInDb) -> anyhow::Result<()> {
        use entity::users;
        users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id))
            .col_expr(
                users::Column::TotpSecretEncrypted,
                sea_orm::prelude::Expr::value(None::<String>),
            )
            .col_expr(
                users::Column::TotpEnabled,
                sea_orm::prelude::Expr::value(false),
            )
            .col_expr(
                users::Column::TotpFailCount,
                sea_orm::prelude::Expr::value(0),
            )
            .col_expr(
                users::Column::TotpLockUntil,
                sea_orm::prelude::Expr::value(None::<i64>),
            )
            .col_expr(
                users::Column::TotpLastStep,
                sea_orm::prelude::Expr::value(None::<i64>),
            )
            .exec(self.orm_db())
            .await?;
        Ok(())
    }

    /// 管理后台用户列表：全部用户 + superusers 组标记 + 2FA 启用状态
    pub async fn list_users_with_2fa(&self) -> anyhow::Result<Vec<AdminUserRow>> {
        use entity::{groups, users, users_groups};
        let all = users::Entity::find()
            .order_by_asc(users::Column::Id)
            .all(self.orm_db())
            .await?;
        let super_ids: std::collections::HashSet<i32> = users_groups::Entity::find()
            .join(JoinType::InnerJoin, users_groups::Relation::Groups.def())
            .filter(groups::Column::Name.eq(SUPERUSERS_GROUP))
            .all(self.orm_db())
            .await?
            .into_iter()
            .map(|ug| ug.user_id)
            .collect();
        Ok(all
            .into_iter()
            .map(|u| AdminUserRow {
                id: u.id,
                username: u.username,
                is_superuser: super_ids.contains(&u.id),
                totp_enabled: u.totp_enabled,
            })
            .collect())
    }

    /// 设备授权判定：未登记设备视为传统模式放行；已登记设备仅 approved 放行。
    pub async fn device_is_authorized(&self, machine_id: Uuid) -> Result<bool, DbErr> {
        let d = self.get_device_by_machine_id(machine_id).await?;
        Ok(match d {
            // ANF 托管模式：未登记设备默认不授权（未知机器不予放行），
            // 需在设备列表中登记为 pending 并由管理员放行后才放行。
            None => false,
            Some(d) => d.status == DEVICE_STATUS_APPROVED,
        })
    }

    /// 首次心跳登记未知设备为 pending（无需邀请码）。已存在则只更新昵称（display_name），
    /// 返回设备记录。这是"无邀请码 + 机器码审核放行"模式的登记入口。
    pub async fn ensure_device_registered(
        &self,
        machine_id: Uuid,
        display_name: Option<&str>,
    ) -> Result<entity::devices::Model, AnfError> {
        use entity::devices;

        let machine = machine_id.to_string();
        let existing = devices::Entity::find()
            .filter(devices::Column::MachineId.eq(&machine))
            .one(self.orm_db())
            .await
            .map_err(AnfError::Db)?;

        if let Some(d) = existing {
            // 仅当调用方提供了非空昵称且与当前不同时才更新显示名。
            if let Some(name) = display_name
                && !name.is_empty()
                && name != d.display_name
            {
                let mut m = d.into_active_model();
                m.display_name = Set(name.to_string());
                m.updated_at = Set(now());
                devices::Entity::update(m)
                    .exec(self.orm_db())
                    .await
                    .map_err(AnfError::Db)?;
                return self
                    .get_device_by_machine_id(machine_id)
                    .await
                    .map_err(AnfError::Db)?
                    .ok_or(AnfError::DeviceNotFound);
            }
            return Ok(d);
        }

        let m = devices::ActiveModel {
            machine_id: Set(machine.clone()),
            display_name: Set(display_name
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| default_display_name(machine_id))),
            status: Set(DEVICE_STATUS_PENDING.to_string()),
            approved_by: Set(None),
            approved_at: Set(None),
            created_at: Set(now()),
            updated_at: Set(now()),
            ..Default::default()
        };
        let res = devices::Entity::insert(m)
            .exec(self.orm_db())
            .await
            .map_err(AnfError::Db)?;
        devices::Entity::find_by_id(res.last_insert_id)
            .one(self.orm_db())
            .await
            .map_err(AnfError::Db)?
            .ok_or(AnfError::DeviceNotFound)
    }

    async fn ensure_user_in_superusers_txn(
        txn: &sea_orm::DatabaseTransaction,
        user_id: UserIdInDb,
    ) -> Result<(), DbErr> {
        use entity::{groups, users_groups};

        let group = groups::Entity::find()
            .filter(groups::Column::Name.eq(SUPERUSERS_GROUP))
            .one(txn)
            .await?
            .ok_or_else(|| DbErr::Custom("superusers 组不存在".to_string()))?;

        let exists = users_groups::Entity::find()
            .filter(users_groups::Column::UserId.eq(user_id))
            .filter(users_groups::Column::GroupId.eq(group.id))
            .one(txn)
            .await?
            .is_some();
        if !exists {
            users_groups::Entity::insert(users_groups::ActiveModel {
                user_id: Set(user_id),
                group_id: Set(group.id),
                ..Default::default()
            })
            .exec(txn)
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn machine() -> Uuid {
        Uuid::new_v4()
    }

    async fn admin_user(db: &Db) -> UserIdInDb {
        db.auto_create_user("admin-tester").await.unwrap().id
    }

    #[tokio::test]
    async fn generate_invite_produces_unique_12_char_codes() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let a = db.generate_invite(admin, 1, None).await.unwrap();
        let b = db.generate_invite(admin, 1, None).await.unwrap();
        assert_eq!(a.code.len(), 12);
        assert_ne!(a.code, b.code);
        assert!(a.enabled);
        assert_eq!(a.used_count, 0);
    }

    #[tokio::test]
    async fn invite_consumed_once_then_rejected() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 1, None).await.unwrap();
        let d1 = db.register_device(&invite.code, machine()).await.unwrap();
        assert_eq!(d1.status, DEVICE_STATUS_PENDING);
        let err = db
            .register_device(&invite.code, machine())
            .await
            .unwrap_err();
        assert!(matches!(err, AnfError::InviteUsedUp));
    }

    #[tokio::test]
    async fn invite_expired_rejected() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let past = chrono::Local::now().fixed_offset() - chrono::Duration::seconds(60);
        let invite = db.generate_invite(admin, 5, Some(past)).await.unwrap();
        let err = db
            .register_device(&invite.code, machine())
            .await
            .unwrap_err();
        assert!(matches!(err, AnfError::InviteExpired));
    }

    #[tokio::test]
    async fn disabled_invite_rejected() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        db.disable_invite(invite.id).await.unwrap();
        let err = db
            .register_device(&invite.code, machine())
            .await
            .unwrap_err();
        assert!(matches!(err, AnfError::InviteNotFound));
    }

    #[tokio::test]
    async fn register_device_defaults_to_pending_with_prefix_name() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let m = machine();
        let d = db.register_device(&invite.code, m).await.unwrap();
        assert_eq!(d.status, DEVICE_STATUS_PENDING);
        assert_eq!(d.display_name, m.simple().to_string()[..8]);
        assert_eq!(d.approved_by, None);
    }

    #[tokio::test]
    async fn re_register_pending_updates_but_approved_stays() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 10, None).await.unwrap();
        let m = machine();
        let d1 = db.register_device(&invite.code, m).await.unwrap();
        let d2 = db.register_device(&invite.code, m).await.unwrap();
        assert_eq!(d1.id, d2.id);
        assert_eq!(d2.status, DEVICE_STATUS_PENDING);

        let approved = db
            .set_device_status(d2.id, DeviceStatus::Approved, admin)
            .await
            .unwrap();
        assert_eq!(approved.status, DEVICE_STATUS_APPROVED);
        assert_eq!(approved.approved_by, Some(admin));

        let d3 = db.register_device(&invite.code, m).await.unwrap();
        assert_eq!(
            d3.status, DEVICE_STATUS_APPROVED,
            "已放行设备重复注册不应降级"
        );
    }

    #[tokio::test]
    async fn device_status_machine_transitions() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let d = db.register_device(&invite.code, machine()).await.unwrap();

        let approved = db
            .set_device_status(d.id, DeviceStatus::Approved, admin)
            .await
            .unwrap();
        assert_eq!(approved.status, DEVICE_STATUS_APPROVED);

        let kicked = db
            .set_device_status(d.id, DeviceStatus::Kicked, admin)
            .await
            .unwrap();
        assert_eq!(kicked.status, DEVICE_STATUS_KICKED);

        let err = db
            .set_device_status(d.id, DeviceStatus::Approved, admin)
            .await
            .unwrap_err();
        assert!(matches!(err, AnfError::InvalidTransition(..)));
    }

    #[tokio::test]
    async fn rejected_is_terminal() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let d = db.register_device(&invite.code, machine()).await.unwrap();
        db.set_device_status(d.id, DeviceStatus::Rejected, admin)
            .await
            .unwrap();
        let err = db
            .set_device_status(d.id, DeviceStatus::Approved, admin)
            .await
            .unwrap_err();
        assert!(matches!(err, AnfError::InvalidTransition(..)));
    }

    #[tokio::test]
    async fn default_list_shows_all_devices_and_reegister_revives() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let m = machine();
        let d = db.register_device(&invite.code, m).await.unwrap();
        assert_eq!(d.status, DEVICE_STATUS_PENDING);

        // 拒绝后默认列表仍显示（全量可见，参考 Tailscale 授权页），便于审计与二次处理。
        db.set_device_status(d.id, DeviceStatus::Rejected, admin)
            .await
            .unwrap();
        let all = db.list_devices(None).await.unwrap();
        assert!(
            all.iter().any(|x| x.id == d.id),
            "默认设备列表应显示全部设备（含已拒绝）"
        );

        // 显式按 rejected 过滤仍可审计。
        let rejected = db.list_devices(Some(DeviceStatus::Rejected)).await.unwrap();
        assert!(
            rejected.iter().any(|x| x.id == d.id),
            "显式查询 rejected 应能看到该设备"
        );

        // 同一机器码重新申请 -> 回到 pending 并再次显示。
        let invite2 = db.generate_invite(admin, 5, None).await.unwrap();
        let d2 = db.register_device(&invite2.code, m).await.unwrap();
        assert_eq!(d2.id, d.id, "同机码重注册应复用同一设备记录");
        assert_eq!(d2.status, DEVICE_STATUS_PENDING);
        let all_after = db.list_devices(None).await.unwrap();
        assert!(
            all_after.iter().any(|x| x.id == d.id),
            "同机码重注册后应再次显示该设备"
        );
    }

    #[tokio::test]
    async fn delete_device_removes_record_and_associations() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let m = machine();
        let d = db.register_device(&invite.code, m).await.unwrap();
        db.update_device(
            d.id,
            None,
            Some(vec!["办公".to_string()]),
            Some(vec!["net-a".to_string()]),
        )
        .await
        .unwrap();

        assert!(db.delete_device(d.id).await.unwrap());
        assert!(db.get_device(d.id).await.unwrap().is_none());
        assert!(db.list_device_tags(d.id).await.unwrap().is_empty());
        assert!(db.list_device_networks(d.id).await.unwrap().is_empty());
        // 删除不存在的记录返回 false，不报错
        assert!(!db.delete_device(99999).await.unwrap());
    }

    #[tokio::test]
    async fn admin_bind_is_idempotent_and_approves_device() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let m = machine();

        let a1 = db.bind_admin_device(m, admin).await.unwrap();
        let a2 = db.bind_admin_device(m, admin).await.unwrap();
        assert_eq!(a1.machine_id, a2.machine_id);
        assert_eq!(a1.user_id, admin);

        let device = db.get_device_by_machine_id(m).await.unwrap().unwrap();
        assert_eq!(device.status, DEVICE_STATUS_APPROVED);
        assert!(db.user_is_superuser(admin).await.unwrap());
        assert!(db.device_is_authorized(m).await.unwrap());
    }

    #[tokio::test]
    async fn unknown_device_is_denied_by_default() {
        // ANF 托管模式：未登记机器默认不放行（需管理员登记放行）。
        let db = Db::memory_db().await;
        assert!(!db.device_is_authorized(machine()).await.unwrap());
    }

    #[tokio::test]
    async fn ensure_device_registered_creates_pending_and_default_name() {
        let db = Db::memory_db().await;
        let m = machine();
        let d = db.ensure_device_registered(m, None).await.unwrap();
        assert_eq!(d.status, DEVICE_STATUS_PENDING);
        assert_eq!(d.display_name, default_display_name(m));
        assert_eq!(d.approved_by, None);
        // 未授权（pending）
        assert!(!db.device_is_authorized(m).await.unwrap());
    }

    #[tokio::test]
    async fn ensure_device_registered_updates_display_name_and_is_idempotent() {
        let db = Db::memory_db().await;
        let m = machine();
        let d1 = db
            .ensure_device_registered(m, Some("小白-办公室"))
            .await
            .unwrap();
        assert_eq!(d1.display_name, "小白-办公室");
        assert_eq!(d1.status, DEVICE_STATUS_PENDING);

        // 再登记同机器：状态不变，昵称更新
        let d2 = db
            .ensure_device_registered(m, Some("小白-新名"))
            .await
            .unwrap();
        assert_eq!(d2.id, d1.id);
        assert_eq!(d2.display_name, "小白-新名");
        assert_eq!(d2.status, DEVICE_STATUS_PENDING);
    }

    #[tokio::test]
    async fn approved_device_registered_again_stays_approved() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let m = machine();
        let d = db
            .ensure_device_registered(m, Some("已放行"))
            .await
            .unwrap();
        db.set_device_status(d.id, DeviceStatus::Approved, admin)
            .await
            .unwrap();
        // 再次登记不应降级
        let d2 = db
            .ensure_device_registered(m, Some("改昵称"))
            .await
            .unwrap();
        assert_eq!(d2.status, DEVICE_STATUS_APPROVED);
        assert_eq!(d2.display_name, "改昵称");
        assert!(db.device_is_authorized(m).await.unwrap());
    }

    #[tokio::test]
    async fn pending_or_rejected_device_is_not_authorized() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let m = machine();
        let d = db.register_device(&invite.code, m).await.unwrap();
        assert!(!db.device_is_authorized(m).await.unwrap());

        db.set_device_status(d.id, DeviceStatus::Rejected, admin)
            .await
            .unwrap();
        assert!(!db.device_is_authorized(m).await.unwrap());
    }

    #[tokio::test]
    async fn update_device_replaces_tags_and_networks() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let invite = db.generate_invite(admin, 5, None).await.unwrap();
        let d = db.register_device(&invite.code, machine()).await.unwrap();

        let updated = db
            .update_device(
                d.id,
                Some("我的 Mac".to_string()),
                Some(vec!["办公".to_string(), "mac".to_string()]),
                Some(vec!["net-a".to_string()]),
            )
            .await
            .unwrap();
        assert_eq!(updated.display_name, "我的 Mac");
        assert_eq!(
            db.list_device_tags(d.id).await.unwrap(),
            vec!["mac", "办公"]
        );
        assert_eq!(db.list_device_networks(d.id).await.unwrap(), vec!["net-a"]);

        db.update_device(d.id, None, Some(vec!["办公".to_string()]), Some(vec![]))
            .await
            .unwrap();
        assert_eq!(db.list_device_tags(d.id).await.unwrap(), vec!["办公"]);
        assert!(db.list_device_networks(d.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn status_operations_on_missing_device_fail() {
        let db = Db::memory_db().await;
        let admin = admin_user(&db).await;
        let err = db
            .set_device_status(9999, DeviceStatus::Approved, admin)
            .await
            .unwrap_err();
        assert!(matches!(err, AnfError::DeviceNotFound));
    }

    async fn make_superuser(db: &Db, name: &str) -> anyhow::Result<UserIdInDb> {
        let uid = db.auto_create_user(name).await?.id;
        let txn = db.orm_db().begin().await?;
        Db::ensure_user_in_superusers_txn(&txn, uid).await?;
        txn.commit().await?;
        Ok(uid)
    }

    #[tokio::test]
    async fn two_factor_state_defaults_and_lifecycle() {
        let db = Db::memory_db().await;
        let uid = admin_user(&db).await;

        let st = db.get_2fa_state(uid).await.unwrap();
        assert!(!st.enabled);
        assert_eq!(st.secret_encrypted, None);
        assert_eq!(st.fail_count, 0);
        assert_eq!(st.lock_until, None);
        assert!(!db.is_2fa_enabled(uid).await.unwrap());

        // setup：写入加密 secret，处于未启用状态
        db.set_totp_secret(uid, "ENC-SECRET".into()).await.unwrap();
        let st = db.get_2fa_state(uid).await.unwrap();
        assert_eq!(st.secret_encrypted.as_deref(), Some("ENC-SECRET"));
        assert!(!st.enabled);

        // enable：启用并记录防重放基线
        db.enable_totp(uid, 123).await.unwrap();
        assert!(db.is_2fa_enabled(uid).await.unwrap());
        let st = db.get_2fa_state(uid).await.unwrap();
        assert_eq!(st.last_step, Some(123));

        // 登录成功推进重放基线
        db.set_2fa_last_step(uid, 456).await.unwrap();
        assert_eq!(db.get_2fa_state(uid).await.unwrap().last_step, Some(456));

        // 重置：全部字段归零
        db.clear_totp(uid).await.unwrap();
        let st = db.get_2fa_state(uid).await.unwrap();
        assert!(!st.enabled);
        assert_eq!(st.secret_encrypted, None);
        assert_eq!(st.fail_count, 0);
        assert_eq!(st.lock_until, None);
        assert_eq!(st.last_step, None);
    }

    #[tokio::test]
    async fn record_2fa_fail_locks_in_ladder_then_clears() {
        let db = Db::memory_db().await;
        let uid = admin_user(&db).await;

        // 第 1~4 次失败不锁
        for _ in 1..=4 {
            assert_eq!(db.record_2fa_fail(uid, 1000).await.unwrap(), None);
        }
        // 第 5 次：锁 10s
        assert_eq!(db.record_2fa_fail(uid, 1000).await.unwrap(), Some(1010));
        let st = db.get_2fa_state(uid).await.unwrap();
        assert_eq!(st.fail_count, 5);
        assert_eq!(st.lock_until, Some(1010));

        // 第 6~9 次失败不重复锁
        for _ in 6..=9 {
            assert_eq!(db.record_2fa_fail(uid, 1000).await.unwrap(), None);
        }
        // 第 10 次：锁 30s
        assert_eq!(db.record_2fa_fail(uid, 1000).await.unwrap(), Some(1030));

        // 验证成功后清零
        db.clear_2fa_fail(uid).await.unwrap();
        let st = db.get_2fa_state(uid).await.unwrap();
        assert_eq!(st.fail_count, 0);
        assert_eq!(st.lock_until, None);
    }

    #[tokio::test]
    async fn list_users_reports_superuser_and_totp_flags() {
        let db = Db::memory_db().await;
        let super1 = make_superuser(&db, "boss-a").await.unwrap();
        let plain = db.auto_create_user("plain-b").await.unwrap().id;
        db.set_totp_secret(plain, "ENC".into()).await.unwrap();
        db.enable_totp(plain, 1).await.unwrap();

        let rows = db.list_users_with_2fa().await.unwrap();
        let boss = rows.iter().find(|r| r.id == super1).unwrap();
        assert!(boss.is_superuser);
        assert!(!boss.totp_enabled);
        let user = rows.iter().find(|r| r.id == plain).unwrap();
        assert!(!user.is_superuser);
        assert!(user.totp_enabled);
    }
}
