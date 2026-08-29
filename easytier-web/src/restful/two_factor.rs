//! ANF 用户 TOTP 两步验证 REST API。
//!
//! 登录两步式流程（Gitea 同款，设计共识 2026-08-29）：
//! 1. `POST /api/v1/auth/login` 密码通过后，若需要 2FA（已启用，或 superuser 强制策略），
//!    写入"待二次验证"半会话（5 分钟超时），响应 `{require_2fa: true}`；
//! 2. `POST /api/v1/auth/2fa/verify` 校验动态码，通过后建立正式会话；
//!    superuser 未绑定时 verify 直接放行并返回 `setup_required=true`，前端强制引导绑定；
//! 3. 绑定：`setup`（生成 secret）→ `enable`（验码启用）；解绑：`disable`（验码关闭）。
//!
//! 防爆破：会话级连续错 5 次作废半会话；账号级每满 5 次失败按 10s→30s→翻倍退避锁定。
//! OIDC 登录路径不走本模块（豁免），但 OIDC 的 superuser 仍受 AdminSession 强制约束。

use axum::{
    Json, Router,
    extract::{Extension, Path},
    http::StatusCode,
    routing::{get, post},
};
use axum_login::{AuthUser, AuthnBackend, login_required};
use serde::Deserialize;
use tower_sessions::Session;

use super::users::{AuthSession, Backend};
use super::{AdminSession, AppStateInner, HttpHandleError, convert_db_error, other_error};
use crate::anf::two_factor as tf;
use crate::db::Db;
use crate::db::anf::AdminUserRow;

const PENDING_2FA_USER_ID: &str = "anf-pending-2fa-user-id";
const PENDING_2FA_EXPIRES_AT: &str = "anf-pending-2fa-expires-at";
const PENDING_2FA_FAILS: &str = "anf-pending-2fa-fails";

/// "待二次验证"半会话有效期
pub const PENDING_2FA_TTL_SECS: i64 = 300;
/// 半会话内动态码连续错误上限，达到即作废半会话
const SESSION_MAX_FAILS: i64 = 5;

/// TOTP 主密钥（启动时加载一次，Extension 注入）
#[derive(Clone)]
pub struct TotpKey(std::sync::Arc<[u8; 32]>);

impl TotpKey {
    pub fn load(db_path: &str) -> anyhow::Result<Self> {
        Ok(Self(std::sync::Arc::new(tf::load_master_key(db_path)?)))
    }

    fn key(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub code: String,
}

// ===== 半会话（"待二次验证"状态）=====

pub async fn set_pending_2fa(session: &Session, user_id: i32, now_ts: i64) {
    let _ = session.insert(PENDING_2FA_USER_ID, user_id).await;
    let _ = session
        .insert(PENDING_2FA_EXPIRES_AT, now_ts + PENDING_2FA_TTL_SECS)
        .await;
    let _ = session.remove::<i64>(PENDING_2FA_FAILS).await;
}

pub async fn pending_2fa_user_id(session: &Session) -> anyhow::Result<Option<i32>> {
    Ok(session.get::<i32>(PENDING_2FA_USER_ID).await?)
}

pub async fn clear_pending_2fa(session: &Session) {
    let _ = session.remove::<i32>(PENDING_2FA_USER_ID).await;
    let _ = session.remove::<i64>(PENDING_2FA_EXPIRES_AT).await;
    let _ = session.remove::<i64>(PENDING_2FA_FAILS).await;
}

// ===== 错误辅助 =====

fn server_error(e: impl std::fmt::Display) -> HttpHandleError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        other_error(format!("{e}")).into(),
    )
}

fn unauthorized(msg: impl ToString) -> HttpHandleError {
    (StatusCode::UNAUTHORIZED, other_error(msg).into())
}

// ===== 路由 =====

pub fn router() -> Router<AppStateInner> {
    // 公开路由：半会话校验依赖 cookie，不要求已登录
    let public = Router::new()
        .route("/api/v1/auth/2fa/pending", get(get_handlers::pending))
        .route("/api/v1/auth/2fa/verify", post(post_handlers::verify));
    // 受保护路由：要求正式登录态；admin 路由额外要求 superuser + 已绑定 2FA
    let protected = Router::new()
        .route("/api/v1/auth/2fa/status", get(get_handlers::status))
        .route("/api/v1/auth/2fa/setup", post(post_handlers::setup))
        .route("/api/v1/auth/2fa/enable", post(post_handlers::enable))
        .route("/api/v1/auth/2fa/disable", post(post_handlers::disable))
        .route("/api/v1/admin/users", get(admin_handlers::list_users))
        .route(
            "/api/v1/admin/users/:id/reset-2fa",
            post(admin_handlers::reset_2fa),
        )
        .route_layer(login_required!(Backend));
    public.merge(protected)
}

mod get_handlers {
    use super::*;

    /// 半会话状态探测（公开）：TwoFactorPage 挂载时刷新页面后恢复流程用
    pub async fn pending(
        session: Session,
        Extension(db): Extension<Db>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        let Some(user_id) = pending_2fa_user_id(&session).await.map_err(server_error)? else {
            return Ok(Json(serde_json::json!({ "pending": false })));
        };
        let expires: i64 = session
            .get(PENDING_2FA_EXPIRES_AT)
            .await
            .map_err(server_error)?
            .unwrap_or(0);
        if tf::unix_now() > expires {
            clear_pending_2fa(&session).await;
            return Ok(Json(serde_json::json!({ "pending": false })));
        }
        let is_superuser = db
            .user_is_superuser(user_id)
            .await
            .map_err(convert_db_error)?;
        let enabled = db.is_2fa_enabled(user_id).await.map_err(server_error)?;
        Ok(Json(serde_json::json!({
            "pending": true,
            "setup_required": is_superuser && !enabled,
        })))
    }

    /// 当前登录用户的 2FA 状态（个人"两步验证"弹窗与前端守卫用）
    pub async fn status(
        auth_session: AuthSession,
        Extension(db): Extension<Db>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        let Some(user) = auth_session.user else {
            return Err(unauthorized("未登录"));
        };
        let is_superuser = db
            .user_is_superuser(user.id())
            .await
            .map_err(convert_db_error)?;
        let state = db.get_2fa_state(user.id()).await.map_err(server_error)?;
        Ok(Json(serde_json::json!({
            "enabled": state.enabled,
            "is_superuser": is_superuser,
            "setup_required": is_superuser && !state.enabled,
        })))
    }
}

mod post_handlers {
    use super::*;

    /// 校验动态码并建立正式会话
    pub async fn verify(
        mut auth_session: AuthSession,
        session: Session,
        Extension(db): Extension<Db>,
        Extension(key): Extension<TotpKey>,
        Json(req): Json<VerifyRequest>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        let now = tf::unix_now();
        let Some(user_id) = pending_2fa_user_id(&session).await.map_err(server_error)? else {
            return Err(unauthorized("无待二次验证会话，请重新登录"));
        };
        let expires: i64 = session
            .get(PENDING_2FA_EXPIRES_AT)
            .await
            .map_err(server_error)?
            .unwrap_or(0);
        if now > expires {
            clear_pending_2fa(&session).await;
            return Err(unauthorized("验证会话已过期，请重新登录"));
        }

        let state = db.get_2fa_state(user_id).await.map_err(server_error)?;
        if let Some(until) = state.lock_until
            && now < until
        {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                other_error(format!("尝试过于频繁，请 {} 秒后再试", until - now + 1)).into(),
            ));
        }

        // 已启用 2FA：必须验证动态码；未启用（superuser 强制绑定流程）：直接放行
        if state.enabled {
            let Some(encrypted) = state.secret_encrypted else {
                return Err(server_error("2FA 状态异常：已启用但缺少 secret"));
            };
            let secret = tf::decrypt_secret(key.key(), &encrypted).map_err(server_error)?;
            let matched =
                tf::verify_code(&secret, &req.code, tf::current_step(now), state.last_step)
                    .map_err(server_error)?;
            let Some(step) = matched else {
                // 会话级：连续错 SESSION_MAX_FAILS 次作废半会话
                let fails: i64 = session
                    .get::<i64>(PENDING_2FA_FAILS)
                    .await
                    .map_err(server_error)?
                    .unwrap_or(0)
                    + 1;
                if fails >= SESSION_MAX_FAILS {
                    clear_pending_2fa(&session).await;
                    return Err(unauthorized("动态码错误次数过多，请重新登录"));
                }
                let _ = session.insert(PENDING_2FA_FAILS, fails).await;
                // 账号级退避（知道密码也无法高频试码）
                let new_lock = db
                    .record_2fa_fail(user_id, now)
                    .await
                    .map_err(server_error)?;
                let msg = match new_lock {
                    Some(until) => {
                        format!("动态码错误，已临时锁定 {} 秒", until - now)
                    }
                    None => "动态码错误".to_string(),
                };
                return Err(unauthorized(msg));
            };
            db.clear_2fa_fail(user_id).await.map_err(server_error)?;
            db.set_2fa_last_step(user_id, step as i64)
                .await
                .map_err(server_error)?;
        }

        let Some(user) = auth_session
            .backend
            .get_user(&user_id)
            .await
            .map_err(server_error)?
        else {
            return Err(server_error("用户不存在"));
        };
        auth_session
            .login(&user)
            .await
            .map_err(|e| server_error(format!("建立会话失败: {e:?}")))?;
        let setup_required = !state.enabled;
        clear_pending_2fa(&session).await;
        tracing::info!("用户 {} 通过两步验证登录", user.id());
        Ok(Json(
            serde_json::json!({ "setup_required": setup_required }),
        ))
    }

    /// 生成 TOTP secret（绑定流程第一步；可重复调用覆盖旧 secret）
    pub async fn setup(
        auth_session: AuthSession,
        Extension(db): Extension<Db>,
        Extension(key): Extension<TotpKey>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        let Some(user) = auth_session.user else {
            return Err(unauthorized("未登录"));
        };
        let secret = tf::generate_secret();
        let encrypted = tf::encrypt_secret(key.key(), &secret).map_err(server_error)?;
        db.set_totp_secret(user.id(), encrypted)
            .await
            .map_err(server_error)?;
        let uri = tf::otpauth_uri(&secret, tf::TOTP_ISSUER, &user.db_user.username);
        tracing::info!("用户 {} 开始绑定两步验证", user.id());
        Ok(Json(
            serde_json::json!({ "secret": secret, "otpauth_url": uri }),
        ))
    }

    /// 验证动态码并启用 2FA（绑定流程第二步）
    pub async fn enable(
        auth_session: AuthSession,
        Extension(db): Extension<Db>,
        Extension(key): Extension<TotpKey>,
        Json(req): Json<VerifyRequest>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        let Some(user) = auth_session.user else {
            return Err(unauthorized("未登录"));
        };
        let now = tf::unix_now();
        let state = db.get_2fa_state(user.id()).await.map_err(server_error)?;
        if let Some(until) = state.lock_until
            && now < until
        {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                other_error(format!("尝试过于频繁，请 {} 秒后再试", until - now + 1)).into(),
            ));
        }
        let Some(encrypted) = state.secret_encrypted else {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error("请先生成绑定二维码（setup）").into(),
            ));
        };
        let secret = tf::decrypt_secret(key.key(), &encrypted).map_err(server_error)?;
        let matched = tf::verify_code(&secret, &req.code, tf::current_step(now), None)
            .map_err(server_error)?;
        let Some(step) = matched else {
            db.record_2fa_fail(user.id(), now)
                .await
                .map_err(server_error)?;
            return Err(unauthorized("动态码错误"));
        };
        db.enable_totp(user.id(), step as i64)
            .await
            .map_err(server_error)?;
        tracing::info!("用户 {} 已启用两步验证", user.id());
        Ok(Json(serde_json::json!({ "enabled": true })))
    }

    /// 验证当前动态码后关闭 2FA
    pub async fn disable(
        auth_session: AuthSession,
        Extension(db): Extension<Db>,
        Extension(key): Extension<TotpKey>,
        Json(req): Json<VerifyRequest>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        let Some(user) = auth_session.user else {
            return Err(unauthorized("未登录"));
        };
        let now = tf::unix_now();
        let state = db.get_2fa_state(user.id()).await.map_err(server_error)?;
        if !state.enabled {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error("两步验证未启用").into(),
            ));
        }
        if let Some(until) = state.lock_until
            && now < until
        {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                other_error(format!("尝试过于频繁，请 {} 秒后再试", until - now + 1)).into(),
            ));
        }
        let Some(encrypted) = state.secret_encrypted else {
            return Err(server_error("2FA 状态异常：已启用但缺少 secret"));
        };
        let secret = tf::decrypt_secret(key.key(), &encrypted).map_err(server_error)?;
        let matched = tf::verify_code(&secret, &req.code, tf::current_step(now), state.last_step)
            .map_err(server_error)?;
        if matched.is_none() {
            db.record_2fa_fail(user.id(), now)
                .await
                .map_err(server_error)?;
            return Err(unauthorized("动态码错误"));
        }
        db.clear_totp(user.id()).await.map_err(server_error)?;
        tracing::info!("用户 {} 已关闭两步验证", user.id());
        Ok(Json(serde_json::json!({ "enabled": false })))
    }
}

mod admin_handlers {
    use super::*;

    /// 用户列表（superuser 专用）
    pub async fn list_users(
        AdminSession(_auth): AdminSession,
        Extension(db): Extension<Db>,
    ) -> Result<Json<Vec<AdminUserRow>>, HttpHandleError> {
        Ok(Json(db.list_users_with_2fa().await.map_err(server_error)?))
    }

    /// 重置指定用户的两步验证（验证器丢失救援）
    pub async fn reset_2fa(
        AdminSession(_auth): AdminSession,
        Extension(db): Extension<Db>,
        Path(user_id): Path<i32>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        if db.get_2fa_state(user_id).await.is_err() {
            return Err((StatusCode::NOT_FOUND, other_error("用户不存在").into()));
        }
        db.clear_totp(user_id).await.map_err(server_error)?;
        tracing::info!("管理员重置了用户 {user_id} 的两步验证");
        Ok(Json(serde_json::json!({ "ok": true })))
    }
}
