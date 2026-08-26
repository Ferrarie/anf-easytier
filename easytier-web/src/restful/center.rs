//! 中心运行信息（管理员）。

use std::sync::Arc;

use axum::{Extension, Json, Router, routing::get};
use serde::Serialize;

use super::{AdminSession, AppStateInner, HttpHandleError};
use crate::CenterInfo;

#[derive(Debug, Serialize)]
pub struct CenterInfoJson {
    pub version: String,
    pub api_server_port: u16,
    pub web_server_port: Option<u16>,
    pub config_server_protocol: String,
    pub config_server_port: u16,
    pub anf_network_name: String,
    pub anf_center_peer_url: Option<String>,
}

impl From<&CenterInfo> for CenterInfoJson {
    fn from(info: &CenterInfo) -> Self {
        Self {
            version: info.version.to_string(),
            api_server_port: info.api_server_port,
            web_server_port: info.web_server_port,
            config_server_protocol: info.config_server_protocol.clone(),
            config_server_port: info.config_server_port,
            anf_network_name: info.anf_network_name.clone(),
            anf_center_peer_url: info.anf_center_peer_url.clone(),
        }
    }
}

pub fn router() -> Router<AppStateInner> {
    Router::new().route("/api/v1/center/info", get(handle_center_info))
}

async fn handle_center_info(
    _admin: AdminSession,
    Extension(center_info): Extension<Arc<CenterInfo>>,
) -> Result<Json<CenterInfoJson>, HttpHandleError> {
    Ok(Json(CenterInfoJson::from(center_info.as_ref())))
}
