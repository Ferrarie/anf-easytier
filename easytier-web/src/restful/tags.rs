//! ANFAGENT-30 M2：tag 管理（管理员）。

use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post},
    Router,
};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::db::{Db, entity};

use super::{AdminSession, AppStateInner, HttpHandleError, convert_db_error, other_error};

#[derive(Debug, Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct TagJson {
    pub id: i32,
    pub name: String,
    pub created_at: DateTime<FixedOffset>,
    pub used_by: usize,
}

impl TagJson {
    async fn from_model(db: &Db, m: entity::tags::Model) -> Result<Self, HttpHandleError> {
        let used_by = db
            .device_tags_usage(&m.name)
            .await
            .map_err(convert_db_error)?;
        Ok(Self {
            id: m.id,
            name: m.name,
            created_at: m.created_at,
            used_by,
        })
    }
}

pub fn router() -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/tags", post(create).get(list))
        .route("/api/v1/tags/:id", delete(remove))
}

async fn create(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Json(req): Json<CreateTagRequest>,
) -> Result<Json<TagJson>, HttpHandleError> {
    let tag = db.create_tag(&req.name).await.map_err(convert_db_error)?;
    Ok(Json(TagJson::from_model(&db, tag).await?))
}

async fn list(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
) -> Result<Json<Vec<TagJson>>, HttpHandleError> {
    let tags = db.list_tags().await.map_err(convert_db_error)?;
    let mut out = Vec::with_capacity(tags.len());
    for t in tags {
        out.push(TagJson::from_model(&db, t).await?);
    }
    Ok(Json(out))
}

async fn remove(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Path(id): Path<i32>,
) -> Result<StatusCode, HttpHandleError> {
    use crate::db::anf_networks::AnfNetError;

    match db.delete_tag(id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(AnfNetError::TagNotFound) => {
            Err((StatusCode::NOT_FOUND, Json::from(other_error("tag 不存在"))))
        }
        Err(AnfNetError::TagInUse) => Err((
            StatusCode::CONFLICT,
            Json::from(other_error("tag 仍被设备使用，无法删除")),
        )),
        Err(AnfNetError::Db(d)) => Err(convert_db_error(d)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(other_error(e.to_string())),
        )),
    }
}
