//! ANFAGENT-30 M1：邀请码管理（管理员）。

use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{delete, post},
};
use axum_login::AuthUser;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::db::{Db, entity};

use super::{AdminSession, AppStateInner, HttpHandleError, convert_db_error, other_error};

#[derive(Debug, Deserialize)]
pub struct CreateInviteRequest {
    #[serde(default = "default_max_uses")]
    pub max_uses: i32,
    pub expires_at: Option<DateTime<FixedOffset>>,
}

fn default_max_uses() -> i32 {
    1
}

#[derive(Debug, Serialize)]
pub struct InviteJson {
    pub id: i32,
    pub code: String,
    pub created_by: i32,
    pub max_uses: i32,
    pub used_count: i32,
    pub expires_at: Option<DateTime<FixedOffset>>,
    pub enabled: bool,
    pub created_at: DateTime<FixedOffset>,
}

impl From<entity::invite_codes::Model> for InviteJson {
    fn from(m: entity::invite_codes::Model) -> Self {
        Self {
            id: m.id,
            code: m.code,
            created_by: m.created_by,
            max_uses: m.max_uses,
            used_count: m.used_count,
            expires_at: m.expires_at,
            enabled: m.enabled,
            created_at: m.created_at,
        }
    }
}

pub fn router() -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/invites", post(create).get(list))
        .route("/api/v1/invites/:id", delete(disable))
}

async fn create(
    admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Json(req): Json<CreateInviteRequest>,
) -> Result<Json<InviteJson>, HttpHandleError> {
    let user = admin
        .0
        .user
        .as_ref()
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json::from(other_error("未登录"))))?;
    let invite = db
        .generate_invite(user.id(), req.max_uses, req.expires_at)
        .await
        .map_err(convert_db_error)?;
    Ok(Json(invite.into()))
}

async fn list(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
) -> Result<Json<Vec<InviteJson>>, HttpHandleError> {
    let invites = db.list_invites().await.map_err(convert_db_error)?;
    Ok(Json(invites.into_iter().map(InviteJson::from).collect()))
}

async fn disable(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Path(id): Path<i32>,
) -> Result<StatusCode, HttpHandleError> {
    db.disable_invite(id).await.map_err(convert_db_error)?;
    Ok(StatusCode::NO_CONTENT)
}
