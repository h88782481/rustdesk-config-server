use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};

use crate::models::{map_platform, ServerConfigDto, VersionCheckRequest, VersionCheckResponse};
use crate::release;
use crate::state::AppState;

/// 客户端启动时拉取 host/key/api。
pub async fn get_config(State(st): State<AppState>) -> Json<ServerConfigDto> {
    let store = st.read_store();
    Json(ServerConfigDto {
        host: store.server.host,
        key: store.server.key,
        api: store.server.api,
    })
}

/// 客户端更新检查:按上报的 os / arch 返回 release 页地址,客户端从中解析版本号自行比较。
/// 平台未启用或没有可用安装包时返回空 url,客户端视为无更新。
pub async fn version_latest(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<VersionCheckRequest>,
) -> Result<Json<VersionCheckResponse>, StatusCode> {
    let origin = request_origin(&headers)?;
    let url = st
        .read_store()
        .release_version(map_platform(&req.os), &req.arch)
        .map(|v| format!("{origin}/rustdesk/rustdesk/releases/tag/v{v}"))
        .unwrap_or_default();
    Ok(Json(VersionCheckResponse { url }))
}

/// release 下的文件:签名清单与签名;其余文件名跳转到对应安装包直链。
pub async fn release_file(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path((release_id, file)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let version = release_id.strip_prefix('v').ok_or(StatusCode::NOT_FOUND)?;
    let store = st.read_store();
    let metadata = || {
        let origin = request_origin(&headers)?;
        release::metadata(&store, &origin, version).ok_or(StatusCode::NOT_FOUND)
    };
    let body = match file.as_str() {
        release::METADATA => metadata()?,
        release::SIGNATURE => release::signature(&st.inner.signing_key, &metadata()?),
        _ => {
            let url = release::package_url(&store, version, &file)
                .ok_or(StatusCode::NOT_FOUND)?
                .map_err(link_error)?;
            return Ok(Redirect::temporary(&url).into_response());
        }
    };
    Ok(([(header::CONTENT_TYPE, "application/json")], body).into_response())
}

/// 浏览器下载(公开,浏览器不带 token):跳转到该平台的下载直链。
pub async fn download(
    State(st): State<AppState>,
    Path(platform): Path<String>,
) -> Result<Redirect, StatusCode> {
    redirect_download(&st, &platform)
}

/// release 页(公开):应用内更新失败时客户端提示用浏览器打开它,按 User-Agent 跳转到对应平台直链。
pub async fn release_page(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Redirect, StatusCode> {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    redirect_download(&st, map_platform(ua))
}

fn redirect_download(st: &AppState, platform: &str) -> Result<Redirect, StatusCode> {
    let url = st
        .read_store()
        .download_url(platform)
        .ok_or(StatusCode::NOT_FOUND)?
        .map_err(link_error)?;
    Ok(Redirect::temporary(&url))
}

/// 直链签名失败(如没填 123 云盘鉴权密钥)是服务端配置问题,记日志便于排查。
fn link_error(e: String) -> StatusCode {
    tracing::error!("{e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

/// 客户端按编译进去的 Config Server 地址访问,Host 即其域名(反向代理须透传 Host)。
fn request_origin(headers: &HeaderMap) -> Result<String, StatusCode> {
    headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|host| format!("https://{host}"))
        .ok_or(StatusCode::BAD_REQUEST)
}
