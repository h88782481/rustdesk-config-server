use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{constant_time_eq, get_cookie};
use crate::models::{now_unix, Store, UpdateEntry};
use crate::state::AppState;

type ApiResult = Result<Json<Value>, (StatusCode, String)>;

#[derive(Deserialize)]
pub struct LoginReq {
    pub password: String,
}

pub async fn login(State(st): State<AppState>, Json(body): Json<LoginReq>) -> Response {
    if constant_time_eq(
        body.password.as_bytes(),
        st.inner.cfg.admin_password.as_bytes(),
    ) {
        let sid = uuid::Uuid::new_v4().to_string();
        st.inner.sessions.write().unwrap().insert(sid.clone());
        let secure = if st.inner.cfg.cookie_secure {
            "; Secure"
        } else {
            ""
        };
        let cookie =
            format!("sid={sid}; HttpOnly; Path=/; SameSite=Strict; Max-Age=604800{secure}");
        let mut resp = Json(json!({"ok": true})).into_response();
        if let Ok(hv) = HeaderValue::from_str(&cookie) {
            resp.headers_mut().insert(header::SET_COOKIE, hv);
        }
        resp
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"ok": false, "error": "密码错误"})),
        )
            .into_response()
    }
}

pub async fn logout(State(st): State<AppState>, req: Request) -> Response {
    if let Some(sid) = get_cookie(req.headers(), "sid") {
        st.inner.sessions.write().unwrap().remove(&sid);
    }
    let mut resp = Json(json!({"ok": true})).into_response();
    if let Ok(hv) = HeaderValue::from_str("sid=; HttpOnly; Path=/; SameSite=Strict; Max-Age=0") {
        resp.headers_mut().insert(header::SET_COOKIE, hv);
    }
    resp
}

pub async fn whoami() -> Json<Value> {
    Json(json!({"ok": true}))
}

pub async fn get_state(State(st): State<AppState>) -> Json<Store> {
    Json(st.read_store())
}

#[derive(Deserialize)]
pub struct ServerReq {
    pub host: String,
    pub key: String,
    pub api: String,
}

pub async fn set_server(State(st): State<AppState>, Json(body): Json<ServerReq>) -> ApiResult {
    st.mutate(|s| {
        s.server.host = body.host.trim().to_string();
        s.server.key = body.key.trim().to_string();
        s.server.api = body.api.trim().to_string();
    })
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    pub platform: String,
    pub enabled: bool,
    pub version: String,
    pub download_url: String,
}

pub async fn set_update(State(st): State<AppState>, Json(body): Json<UpdateReq>) -> ApiResult {
    let platform = body.platform.trim().to_string();
    if platform.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "platform is required".to_string()));
    }
    st.mutate(|s| {
        let now = now_unix();
        if let Some(u) = s.updates.iter_mut().find(|u| u.platform == platform) {
            u.enabled = body.enabled;
            u.version = body.version.trim().to_string();
            u.download_url = body.download_url.trim().to_string();
            u.updated_at = now;
        } else {
            s.updates.push(UpdateEntry {
                platform,
                enabled: body.enabled,
                version: body.version.trim().to_string(),
                download_url: body.download_url.trim().to_string(),
                updated_at: now,
            });
        }
    })
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({"ok": true})))
}
