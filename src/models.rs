use serde::{Deserialize, Serialize};

/// 下发给客户端的服务端配置(host/key/api)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub api: String,
}

/// 每个平台一条更新记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEntry {
    pub platform: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub download_url: String,
    #[serde(default)]
    pub updated_at: u64,
}

/// 持久化到磁盘的全部业务数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub updates: Vec<UpdateEntry>,
}

pub const PLATFORMS: [&str; 4] = ["windows", "macos", "linux", "android"];

impl Store {
    /// 确保四个基础平台都有一条记录(便于面板展示)。
    pub fn ensure_platforms(&mut self) {
        for p in PLATFORMS {
            if !self.updates.iter().any(|u| u.platform == p) {
                self.updates.push(UpdateEntry {
                    platform: p.to_string(),
                    enabled: false,
                    version: String::new(),
                    download_url: String::new(),
                    updated_at: 0,
                });
            }
        }
    }
}

/// 对应 rustdesk `hbb_common::version_check_request` 发出的请求体。
#[derive(Debug, Default, Deserialize)]
pub struct VersionCheckRequest {
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub os_version: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub device_id: Vec<u8>,
    #[serde(default)]
    pub typ: String,
}

/// 返回给客户端。`url` 兼容原字段;`version` 供 patched 客户端做版本比较。
#[derive(Debug, Default, Serialize)]
pub struct VersionCheckResponse {
    pub url: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ServerConfigDto {
    pub host: String,
    pub key: String,
    pub api: String,
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 把客户端上报的 os 字段映射到我们的平台键。
pub fn map_platform(os: &str) -> &'static str {
    let o = os.to_lowercase();
    if o.contains("windows") {
        "windows"
    } else if o.contains("android") {
        "android"
    } else if o.contains("ios") || o.contains("iphone") || o.contains("ipad") {
        "ios"
    } else if o.contains("mac") || o.contains("darwin") || o.contains("os x") {
        "macos"
    } else {
        "linux"
    }
}
