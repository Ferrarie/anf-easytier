//! ANFAGENT-30 M1：设备注册（公开）/ 审批与分配（管理员）。

use axum::{
    Json,
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Router,
};
use axum_login::AuthUser;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::anf::config::{
    AnfConfigTemplate, reconcile_device_configs, revoke_device_configs,
};
use crate::client_manager::ClientManager;
use crate::db::{Db, anf::{AnfError, DeviceStatus}, entity};
use crate::FeatureFlags;

use super::{AdminSession, AppStateInner, HttpHandleError, convert_db_error, other_error};

#[derive(Debug, Deserialize)]
pub struct RegisterDeviceRequest {
    pub invite_code: String,
    pub machine_id: String,
}

#[derive(Debug, Serialize)]
pub struct DeviceJson {
    pub id: i32,
    pub machine_id: String,
    pub display_name: String,
    pub status: String,
    pub approved_by: Option<i32>,
    pub approved_at: Option<DateTime<FixedOffset>>,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    pub tags: Vec<String>,
    pub networks: Vec<String>,
}

impl DeviceJson {
    pub(crate) async fn from_model(
        db: &Db,
        m: entity::devices::Model,
    ) -> Result<Self, HttpHandleError> {
        let tags = db.list_device_tags(m.id).await.map_err(convert_db_error)?;
        let networks = db
            .list_device_networks(m.id)
            .await
            .map_err(convert_db_error)?;
        Ok(Self {
            id: m.id,
            machine_id: m.machine_id,
            display_name: m.display_name,
            status: m.status,
            approved_by: m.approved_by,
            approved_at: m.approved_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
            tags,
            networks,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateDeviceRequest {
    pub display_name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub networks: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ListDevicesQuery {
    pub status: Option<String>,
}

fn map_anf_error(e: AnfError) -> HttpHandleError {
    let status = match e {
        AnfError::InviteNotFound | AnfError::InviteExpired | AnfError::InviteUsedUp => {
            axum::http::StatusCode::FORBIDDEN
        }
        AnfError::DeviceNotFound => axum::http::StatusCode::NOT_FOUND,
        AnfError::UserNotFound => axum::http::StatusCode::NOT_FOUND,
        AnfError::InvalidTransition(..) => axum::http::StatusCode::CONFLICT,
        AnfError::Db(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json::from(other_error(e.to_string())))
}

/// 公开路由：设备凭邀请码注册。
pub fn public_router() -> Router<AppStateInner> {
    Router::new().route("/api/v1/devices/register", post(register))
}

/// 管理员路由：设备列表 / 审批 / 拒绝 / 踢出 / 改名 / 分配。
pub fn admin_router() -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/devices", get(list))
        .route("/api/v1/devices/:id/approve", post(approve))
        .route("/api/v1/devices/:id/reject", post(reject))
        .route("/api/v1/devices/:id/kick", post(kick))
        .route(
            "/api/v1/devices/:id",
            delete(remove).patch(update),
        )
}

async fn register(
    axum::Extension(db): axum::Extension<Db>,
    Json(req): Json<RegisterDeviceRequest>,
) -> Result<Json<DeviceJson>, HttpHandleError> {
    let machine_id: Uuid = req.machine_id.parse().map_err(|_| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json::from(other_error(format!("machine_id 不是合法 UUID: {}", req.machine_id))),
        )
    })?;
    let device = db
        .register_device(&req.invite_code, machine_id)
        .await
        .map_err(map_anf_error)?;
    Ok(Json(DeviceJson::from_model(&db, device).await?))
}

async fn list(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Query(query): Query<ListDevicesQuery>,
) -> Result<Json<Vec<DeviceJson>>, HttpHandleError> {
    let status = match query.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(DeviceStatus::from_str(s).ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json::from(other_error(format!("未知的设备状态: {s}"))),
            )
        })?),
    };
    let devices = db.list_devices(status).await.map_err(convert_db_error)?;
    let mut out = Vec::with_capacity(devices.len());
    for d in devices {
        out.push(DeviceJson::from_model(&db, d).await?);
    }
    Ok(Json(out))
}

async fn approve(
    admin: AdminSession,
    db: axum::Extension<Db>,
    State(client_mgr): State<AppStateInner>,
    axum::Extension(feature_flags): axum::Extension<Arc<FeatureFlags>>,
    Path(id): Path<i32>,
) -> Result<Json<DeviceJson>, HttpHandleError> {
    set_status_impl(
        admin,
        db,
        client_mgr,
        feature_flags,
        id,
        DeviceStatus::Approved,
    )
    .await
}

async fn reject(
    admin: AdminSession,
    db: axum::Extension<Db>,
    State(client_mgr): State<AppStateInner>,
    axum::Extension(feature_flags): axum::Extension<Arc<FeatureFlags>>,
    Path(id): Path<i32>,
) -> Result<Json<DeviceJson>, HttpHandleError> {
    set_status_impl(
        admin,
        db,
        client_mgr,
        feature_flags,
        id,
        DeviceStatus::Rejected,
    )
    .await
}

async fn kick(
    admin: AdminSession,
    db: axum::Extension<Db>,
    State(client_mgr): State<AppStateInner>,
    axum::Extension(feature_flags): axum::Extension<Arc<FeatureFlags>>,
    Path(id): Path<i32>,
) -> Result<Json<DeviceJson>, HttpHandleError> {
    set_status_impl(
        admin,
        db,
        client_mgr,
        feature_flags,
        id,
        DeviceStatus::Kicked,
    )
    .await
}

async fn set_status_impl(
    admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    client_mgr: Arc<ClientManager>,
    feature_flags: Arc<FeatureFlags>,
    id: i32,
    status: DeviceStatus,
) -> Result<Json<DeviceJson>, HttpHandleError> {
    let actor = admin.0.user.as_ref().map(|u| u.id()).unwrap_or_default();
    let device = db
        .set_device_status(id, status, actor)
        .await
        .map_err(map_anf_error)?;
    let machine_id = device.machine_id.parse::<Uuid>().map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(other_error(format!("设备机器码非法: {e}"))),
        )
    })?;
    let template = AnfConfigTemplate::new(
        &feature_flags.anf_network_name,
        feature_flags.anf_network_secret.clone(),
        feature_flags.anf_center_peer_url.clone(),
    );
    let result = match status {
        DeviceStatus::Approved => {
            reconcile_device_configs(
                &client_mgr,
                &db,
                &feature_flags.anf_center_user,
                machine_id,
                &template,
            )
            .await
        }
        DeviceStatus::Rejected | DeviceStatus::Kicked => {
            revoke_device_configs(
                &client_mgr,
                &db,
                &feature_flags.anf_center_user,
                machine_id,
            )
            .await
        }
        _ => Ok(()),
    };
    if let Err(e) = result {
        return Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(other_error(format!("状态已更新但配置下发失败: {e}"))),
        ));
    }
    Ok(Json(DeviceJson::from_model(&db, device).await?))
}

async fn update(
    admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    State(client_mgr): State<AppStateInner>,
    axum::Extension(feature_flags): axum::Extension<Arc<FeatureFlags>>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateDeviceRequest>,
) -> Result<Json<DeviceJson>, HttpHandleError> {
    let _ = admin;
    let device = db
        .update_device(id, req.display_name, req.tags, req.networks)
        .await
        .map_err(map_anf_error)?;
    let status = DeviceStatus::from_str(&device.status).unwrap_or(DeviceStatus::Pending);
    if status == DeviceStatus::Approved {
        let machine_id = device.machine_id.parse::<Uuid>().map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("设备机器码非法: {e}"))),
            )
        })?;
        let template = AnfConfigTemplate::new(
            &feature_flags.anf_network_name,
            feature_flags.anf_network_secret.clone(),
            feature_flags.anf_center_peer_url.clone(),
        );
        reconcile_device_configs(
            &client_mgr,
            &db,
            &feature_flags.anf_center_user,
            machine_id,
            &template,
        )
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("分配已保存但配置下发失败: {e}"))),
            )
        })?;
    }
    Ok(Json(DeviceJson::from_model(&db, device).await?))
}

/// 删除设备（tailscale 授权页的“移除机器”语义）。已放行设备先撤销托管配置并断开会话。
async fn remove(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    State(client_mgr): State<AppStateInner>,
    axum::Extension(feature_flags): axum::Extension<Arc<FeatureFlags>>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, HttpHandleError> {
    let device = db
        .get_device(id)
        .await
        .map_err(convert_db_error)?
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json::from(other_error("设备不存在".to_string())),
            )
        })?;
    let status = DeviceStatus::from_str(&device.status).unwrap_or(DeviceStatus::Pending);
    if status == DeviceStatus::Approved {
        let machine_id = device.machine_id.parse::<Uuid>().map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("设备机器码非法: {e}"))),
            )
        })?;
        revoke_device_configs(
            &client_mgr,
            &db,
            &feature_flags.anf_center_user,
            machine_id,
        )
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("撤销配置失败: {e}"))),
            )
        })?;
    }
    let removed = db.delete_device(id).await.map_err(convert_db_error)?;
    Ok(Json(serde_json::json!({ "removed": removed })))
}
