//! ANFAGENT-30 M2：ACL 规则管理（管理员）。

use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post},
    Router,
};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::db::{Db, anf_networks::NewAclRule};

use super::{AdminSession, AppStateInner, HttpHandleError, other_error};

#[derive(Debug, Deserialize)]
pub struct RuleRequest {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub source_tags: Vec<String>,
    #[serde(default)]
    pub destination_tags: Vec<String>,
    #[serde(default = "default_any")]
    pub protocol: String,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default = "default_drop")]
    pub action: String,
    #[serde(default)]
    pub priority: u32,
}

fn default_true() -> bool {
    true
}

fn default_any() -> String {
    "any".to_string()
}

fn default_drop() -> String {
    "drop".to_string()
}

#[derive(Debug, Serialize)]
pub struct RuleJson {
    pub id: i32,
    pub network_inst_id: String,
    pub name: String,
    pub enabled: bool,
    pub source_tags: Vec<String>,
    pub destination_tags: Vec<String>,
    pub protocol: String,
    pub ports: Vec<String>,
    pub action: String,
    pub priority: i32,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
}

impl From<crate::db::entity::acl_rules::Model> for RuleJson {
    fn from(m: crate::db::entity::acl_rules::Model) -> Self {
        Self {
            id: m.id,
            network_inst_id: m.network_inst_id,
            name: m.name,
            enabled: m.enabled,
            source_tags: serde_json::from_str(&m.source_tags).unwrap_or_default(),
            destination_tags: serde_json::from_str(&m.destination_tags).unwrap_or_default(),
            protocol: m.protocol,
            ports: serde_json::from_str(&m.ports).unwrap_or_default(),
            action: m.action,
            priority: m.priority,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

fn map_err(e: crate::db::anf_networks::AnfNetError) -> HttpHandleError {
    use crate::db::anf_networks::AnfNetError;
    let status = match e {
        AnfNetError::RuleNotFound | AnfNetError::NetworkNotFound => StatusCode::NOT_FOUND,
        AnfNetError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json::from(other_error(e.to_string())))
}

pub fn router() -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/networks/:id/rules", post(create).get(list))
        .route("/api/v1/networks/:id/rules/:ruleId", delete(remove))
}

async fn create(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Path(id): Path<String>,
    Json(req): Json<RuleRequest>,
) -> Result<Json<RuleJson>, HttpHandleError> {
    let rule = db
        .create_acl_rule(&NewAclRule {
            network_inst_id: id,
            name: req.name,
            enabled: req.enabled,
            source_tags: req.source_tags,
            destination_tags: req.destination_tags,
            protocol: req.protocol,
            ports: req.ports,
            action: req.action,
            priority: req.priority,
        })
        .await
        .map_err(map_err)?;
    Ok(Json(rule.into()))
}

async fn list(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Path(id): Path<String>,
) -> Result<Json<Vec<RuleJson>>, HttpHandleError> {
    let rules = db
        .list_acl_rules(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("{e:?}"))),
            )
        })?;
    Ok(Json(rules.into_iter().map(RuleJson::from).collect()))
}

async fn remove(
    _admin: AdminSession,
    axum::Extension(db): axum::Extension<Db>,
    Path((_id, rule_id)): Path<(String, i32)>,
) -> Result<StatusCode, HttpHandleError> {
    db.delete_acl_rule(rule_id).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}
