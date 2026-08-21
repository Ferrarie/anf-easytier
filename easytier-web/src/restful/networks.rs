//! ANFAGENT-30 M2：网络实例管理（管理员）。

use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post},
    Router,
};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::db::{Db, anf_networks::AnfNetError, entity};

use super::{AdminSession, AppStateInner, HttpHandleError, convert_db_error, other_error};

#[derive(Debug, Deserialize)]
pub struct CreateNetworkRequest {
    pub name: String,
    pub cidr: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NetworkJson {
    pub id: String,
    pub name: String,
    pub cidr: Option<String>,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    pub device_count: usize,
}

impl NetworkJson {
    async fn from_model(
        db: &Db,
        m: entity::network_instances::Model,
    ) -> Result<Self, HttpHandleError> {
        let device_count = db
            .list_network_devices(&m.id)
            .await
            .map_err(convert_db_error)?
            .len();
        Ok(Self {
            id: m.id,
            name: m.name,
            cidr: m.cidr,
            created_at: m.created_at,
            updated_at: m.updated_at,
            device_count,
        })
    }
}

fn map_err(e: AnfNetError) -> HttpHandleError {
    let status = match e {
        AnfNetError::NetworkNotFound | AnfNetError::TagNotFound | AnfNetError::RuleNotFound => {
            StatusCode::NOT_FOUND
        }
        AnfNetError::NetworkInUse | AnfNetError::TagInUse => StatusCode::CONFLICT,
        AnfNetError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        AnfNetError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json::from(other_error(e.to_string())))
}

pub fn router() -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/networks", post(create).get(list))
        .route("/api/v1/networks/:id", delete(remove))
        .route("/api/v1/networks/:id/devices", get(list_members))
}

async fn create(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Json(req): Json<CreateNetworkRequest>,
) -> Result<Json<NetworkJson>, HttpHandleError> {
    let net = db
        .create_network(&req.name, req.cidr)
        .await
        .map_err(convert_db_error)?;
    Ok(Json(NetworkJson::from_model(&db, net).await?))
}

async fn list(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
) -> Result<Json<Vec<NetworkJson>>, HttpHandleError> {
    let nets = db.list_networks().await.map_err(convert_db_error)?;
    let mut out = Vec::with_capacity(nets.len());
    for n in nets {
        out.push(NetworkJson::from_model(&db, n).await?);
    }
    Ok(Json(out))
}

async fn remove(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Path(id): Path<String>,
) -> Result<StatusCode, HttpHandleError> {
    db.delete_network(&id).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_members(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::restful::devices::DeviceJson>>, HttpHandleError> {
    use crate::restful::devices::DeviceJson;

    if db.get_network(&id).await.map_err(convert_db_error)?.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json::from(other_error("网络实例不存在")),
        ));
    }
    let devices = db
        .list_network_devices(&id)
        .await
        .map_err(convert_db_error)?;
    let mut out = Vec::with_capacity(devices.len());
    for d in devices {
        out.push(DeviceJson::from_model(&db, d).await?);
    }
    Ok(Json(out))
}
