use axum::{
    Extension, Router,
    http::StatusCode,
    routing::{get, post, put},
};
use axum_login::AuthUser;
use axum_login::login_required;
use axum_messages::Message;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::anf::two_factor as tf;
use crate::db::Db;
use crate::restful::users::Backend;

use std::sync::Arc;

use crate::FeatureFlags;

use super::{
    AppStateInner, convert_db_error,
    two_factor::set_pending_2fa,
    users::{AuthSession, Credentials},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginResult {
    messages: Vec<Message>,
}

pub fn router() -> Router<AppStateInner> {
    let r = Router::new()
        .route("/api/v1/auth/password", put(self::put::change_password))
        .route(
            "/api/v1/auth/check_login_status",
            get(self::get::check_login_status),
        )
        .route_layer(login_required!(Backend));
    Router::new()
        .merge(r)
        .route("/api/v1/auth/login", post(self::post::login))
        .route("/api/v1/auth/logout", get(self::get::logout))
        .route("/api/v1/auth/captcha", get(self::get::get_captcha))
        .route("/api/v1/auth/register", post(self::post::register))
}

mod put {
    use axum::Json;
    use axum_login::AuthUser;
    use easytier::proto::common::Void;

    use crate::restful::{HttpHandleError, other_error, users::ChangePassword};

    use super::*;

    pub async fn change_password(
        mut auth_session: AuthSession,
        Json(req): Json<ChangePassword>,
    ) -> Result<Json<Void>, HttpHandleError> {
        if let Err(e) = auth_session
            .backend
            .change_password(auth_session.user.as_ref().unwrap().id(), &req)
            .await
        {
            tracing::error!("Failed to change password: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("{:?}", e))),
            ));
        }

        let _ = auth_session.logout().await;

        Ok(Void::default().into())
    }
}

mod post {
    use axum::{Json, extract::Extension};
    use easytier::proto::common::Void;

    use crate::restful::{
        HttpHandleError,
        captcha::extension::{CaptchaUtil, axum_tower_sessions::CaptchaAxumTowerSessionStaticExt},
        other_error,
        users::RegisterNewUser,
    };

    use super::*;

    pub async fn login(
        mut auth_session: AuthSession,
        session: Session,
        Extension(db): Extension<Db>,
        Json(creds): Json<Credentials>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        let user = match auth_session.authenticate(creds.clone()).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json::from(other_error("Invalid credentials")),
                ));
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json::from(other_error(format!("{:?}", e))),
                ));
            }
        };

        // 两步验证：已启用 TOTP 的用户必须过动态码；superuser 强制策略下即使
        // 未绑定也走半会话（verify 放行后由前端强制引导绑定）
        let state = db.get_2fa_state(user.id()).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("{e}"))),
            )
        })?;
        let is_superuser = db
            .user_is_superuser(user.id())
            .await
            .map_err(convert_db_error)?;
        if state.enabled || is_superuser {
            set_pending_2fa(&session, user.id(), tf::unix_now()).await;
            return Ok(Json(serde_json::json!({ "require_2fa": true })));
        }

        if let Err(e) = auth_session.login(&user).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("{:?}", e))),
            ));
        }

        Ok(Json(serde_json::json!({})))
    }

    pub async fn register(
        Extension(feature_flags): Extension<Arc<FeatureFlags>>,
        auth_session: AuthSession,
        captcha_session: tower_sessions::Session,
        Json(req): Json<RegisterNewUser>,
    ) -> Result<Json<Void>, HttpHandleError> {
        // Check if registration is disabled
        if feature_flags.disable_registration {
            tracing::warn!("Registration attempt blocked: registration is disabled");
            return Err((
                StatusCode::FORBIDDEN,
                other_error("Registration is disabled").into(),
            ));
        }

        // 调用CaptchaUtil的静态方法验证验证码是否正确
        if !CaptchaUtil::ver(&req.captcha, &captcha_session).await {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error(format!("captcha verify error, input: {}", req.captcha)).into(),
            ));
        }

        if let Err(e) = auth_session.backend.register_new_user(&req).await {
            tracing::error!("Failed to register new user: {:?}", e);
            return Err((
                StatusCode::BAD_REQUEST,
                other_error(format!("{:?}", e)).into(),
            ));
        }

        Ok(Void::default().into())
    }
}

mod get {
    use crate::restful::{
        HttpHandleError,
        captcha::{
            NewCaptcha as _,
            builder::spec::SpecCaptcha,
            extension::{CaptchaUtil, axum_tower_sessions::CaptchaAxumTowerSessionExt as _},
        },
        other_error,
    };
    use axum::{Json, response::Response};
    use easytier::proto::common::Void;
    use tower_sessions::Session;

    use super::*;

    pub async fn logout(mut auth_session: AuthSession) -> Result<Json<Void>, HttpHandleError> {
        match auth_session.logout().await {
            Ok(_) => Ok(Json(Void::default())),
            Err(e) => {
                tracing::error!("Failed to logout: {:?}", e);
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json::from(other_error(format!("{:?}", e))),
                ))
            }
        }
    }

    pub async fn get_captcha(session: Session) -> Result<Response, HttpHandleError> {
        let mut captcha: CaptchaUtil<SpecCaptcha> = CaptchaUtil::with_size_and_len(127, 48, 4);
        match captcha.out(&session).await {
            Ok(response) => Ok(response),
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("{:?}", e))),
            )),
        }
    }

    pub async fn check_login_status(
        auth_session: AuthSession,
        Extension(db): Extension<Db>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        let Some(user) = auth_session.user else {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json::from(other_error("Not logged in")),
            ));
        };
        // HTTP 语义不变（200 已登录 / 401 未登录），响应体供前端守卫判断
        // superuser 是否需要强制绑定两步验证
        let is_superuser = db
            .user_is_superuser(user.id())
            .await
            .map_err(convert_db_error)?;
        let enabled = db.is_2fa_enabled(user.id()).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("{e}"))),
            )
        })?;
        Ok(Json(
            serde_json::json!({ "require_two_factor_setup": is_superuser && !enabled }),
        ))
    }
}
