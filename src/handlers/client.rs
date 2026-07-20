use axum::{extract::State, Json};

use crate::models::{map_platform, ServerConfigDto, VersionCheckRequest, VersionCheckResponse};
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

/// 客户端更新检查。按平台匹配记录,下载链接支持 `{version}` / `{arch}` 占位符。
/// 无匹配或未启用时返回空 url,客户端据此视为"无更新"。
pub async fn version_latest(
    State(st): State<AppState>,
    Json(req): Json<VersionCheckRequest>,
) -> Json<VersionCheckResponse> {
    let platform = map_platform(&req.os);
    let store = st.read_store();
    let resp = store
        .updates
        .iter()
        .find(|u| u.platform == platform)
        .filter(|u| u.enabled && !u.version.is_empty() && !u.download_url.is_empty())
        .map(|u| {
            let url = u
                .download_url
                .replace("{version}", &u.version)
                .replace("{arch}", &req.arch);
            VersionCheckResponse {
                url,
                version: u.version.clone(),
            }
        })
        .unwrap_or_default();
    Json(resp)
}
