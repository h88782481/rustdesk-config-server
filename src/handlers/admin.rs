use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{constant_time_eq, get_cookie};
use crate::models::{is_valid_version, now_unix, PLATFORMS};
use crate::release;
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

pub async fn get_state(State(st): State<AppState>) -> Json<Value> {
    let store = st.read_store();
    Json(json!({
        "server": store.server,
        "updates": store.updates,
        "update_public_key": st.update_public_key(),
        "pan123_auth_key": store.pan123_auth_key,
    }))
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
pub struct Pan123Req {
    pub auth_key: String,
}

/// 保存 123 云盘直链鉴权密钥。
pub async fn set_pan123(State(st): State<AppState>, Json(body): Json<Pan123Req>) -> ApiResult {
    st.mutate(|s| s.pan123_auth_key = body.auth_key.trim().to_string())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    pub platform: String,
    pub enabled: bool,
    pub version: String,
    #[serde(default)]
    pub download_url: String,
    #[serde(default)]
    pub download_pan123_auth: bool,
    #[serde(default)]
    pub packages: Vec<PackageReq>,
}

#[derive(Deserialize)]
pub struct PackageReq {
    pub arch: String,
    pub url: String,
    #[serde(default)]
    pub pan123_auth: bool,
}

/// 保存某平台的更新设置,开了 123 鉴权的直链须能签名。版本号或直链变了的安装包清空同步结果,需重新同步。
pub async fn set_update(State(st): State<AppState>, Json(body): Json<UpdateReq>) -> ApiResult {
    if !PLATFORMS.contains(&body.platform.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "unknown platform".to_string()));
    }
    let version = body.version.trim();
    if !version.is_empty() && !is_valid_version(version) {
        return Err((
            StatusCode::BAD_REQUEST,
            "版本号格式应为 1.5.3 或 1.5.3+1".to_string(),
        ));
    }
    let store = st.read_store();
    for (url, pan123_auth) in body
        .packages
        .iter()
        .map(|p| (p.url.trim(), p.pan123_auth))
        .chain([(body.download_url.trim(), body.download_pan123_auth)])
    {
        if !url.is_empty() {
            store
                .direct_link(url, pan123_auth)
                .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        }
    }
    st.mutate(|s| {
        let Some(u) = s.updates.iter_mut().find(|u| u.platform == body.platform) else {
            return;
        };
        let version_changed = u.version != version;
        for p in &mut u.packages {
            let req = body.packages.iter().find(|r| r.arch == p.arch);
            let url = req.map_or(p.url.as_str(), |r| r.url.trim()).to_string();
            if version_changed || p.url != url {
                p.url = url;
                p.size = 0;
                p.sha256.clear();
            }
            p.pan123_auth = req.map_or(p.pan123_auth, |r| r.pan123_auth);
        }
        u.enabled = body.enabled;
        u.version = version.to_string();
        u.download_url = body.download_url.trim().to_string();
        u.download_pan123_auth = body.download_pan123_auth;
        u.updated_at = now_unix();
    })
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct SyncReq {
    pub platform: String,
    pub arch: String,
}

/// 同步安装包:按已保存的直链流式下载,记下大小与 sha256(数据不落盘)。
/// 在独立任务里执行,面板请求被反向代理超时断开时同步照样完成并保存。
pub async fn sync_package(State(st): State<AppState>, Json(body): Json<SyncReq>) -> ApiResult {
    let store = st.read_store();
    let (version, url, pan123_auth) = store
        .updates
        .iter()
        .find(|u| u.platform == body.platform)
        .and_then(|u| {
            let p = u.packages.iter().find(|p| p.arch == body.arch)?;
            Some((u.version.clone(), p.url.clone(), p.pan123_auth))
        })
        .filter(|(_, url, _)| !url.is_empty())
        .ok_or((StatusCode::BAD_REQUEST, "请先填写安装包直链".to_string()))?;
    let link = store
        .direct_link(&url, pan123_auth)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    tokio::spawn(async move {
        let (size, sha256) = release::digest(&link)
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("下载失败: {e}")))?;
        let mut synced = false;
        st.mutate(|s| {
            let Some(u) = s
                .updates
                .iter_mut()
                .find(|u| u.platform == body.platform && u.version == version)
            else {
                return;
            };
            if let Some(p) = u
                .packages
                .iter_mut()
                .find(|p| p.arch == body.arch && p.url == url)
            {
                p.size = size;
                p.sha256 = sha256;
                u.updated_at = now_unix();
                synced = true;
            }
        })
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if synced {
            Ok(Json(json!({"ok": true})))
        } else {
            Err((
                StatusCode::CONFLICT,
                "同步期间版本号或直链被修改,请重新同步".to_string(),
            ))
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
}
