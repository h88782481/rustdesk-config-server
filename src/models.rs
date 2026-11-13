use serde::{Deserialize, Serialize};

use crate::pan123;

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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateEntry {
    pub platform: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub version: String,
    /// Linux / Android 的浏览器下载直链,支持 `{version}`。
    #[serde(default)]
    pub download_url: String,
    /// 下载直链开了 123 云盘鉴权。
    #[serde(default)]
    pub download_pan123_auth: bool,
    /// Windows / macOS 的应用内更新安装包,每个架构一个。
    #[serde(default)]
    pub packages: Vec<Package>,
    #[serde(default)]
    pub updated_at: u64,
}

/// 应用内更新安装包:直链,以及「同步」得到的大小与 sha256(未同步时为空)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Package {
    pub arch: String,
    #[serde(default)]
    pub url: String,
    /// 直链开了 123 云盘鉴权。
    #[serde(default)]
    pub pan123_auth: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
}

impl Package {
    pub fn is_synced(&self) -> bool {
        !self.sha256.is_empty()
    }
}

/// 持久化到磁盘的全部业务数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub updates: Vec<UpdateEntry>,
    /// 123 云盘直链鉴权密钥。
    #[serde(default)]
    pub pan123_auth_key: String,
}

pub const PLATFORMS: [&str; 4] = ["windows", "macos", "linux", "android"];

/// 有应用内更新的平台:安装包格式与架构,与客户端查找清单时用的 format / arch 一致。
/// Linux / Android 没有应用内更新,只用浏览器下载直链。
pub fn package_spec(platform: &str) -> Option<(&'static str, &'static [&'static str])> {
    match platform {
        "windows" => Some(("exe", &["x86_64", "x86"])),
        "macos" => Some(("dmg", &["x86_64", "aarch64"])),
        _ => None,
    }
}

impl Store {
    /// 确保四个基础平台及其安装包架构都有记录(便于面板展示)。
    pub fn ensure_platforms(&mut self) {
        for p in PLATFORMS {
            if !self.updates.iter().any(|u| u.platform == p) {
                self.updates.push(UpdateEntry {
                    platform: p.to_string(),
                    ..Default::default()
                });
            }
        }
        for u in &mut self.updates {
            if let Some((_, arches)) = package_spec(&u.platform) {
                for arch in arches {
                    if !u.packages.iter().any(|p| p.arch == *arch) {
                        u.packages.push(Package {
                            arch: arch.to_string(),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    /// 已启用且填了版本号的平台记录。
    fn active(&self, platform: &str) -> Option<&UpdateEntry> {
        self.updates
            .iter()
            .find(|u| u.platform == platform && u.enabled && !u.version.is_empty())
    }

    /// 客户端可更新到的版本:Windows / macOS 需要该架构的安装包已同步,Linux / Android 需要下载直链。
    pub fn release_version(&self, platform: &str, arch: &str) -> Option<&str> {
        let u = self.active(platform)?;
        let ready = match package_spec(platform) {
            Some(_) => u.packages.iter().any(|p| p.arch == arch && p.is_synced()),
            None => !u.download_url.is_empty(),
        };
        ready.then_some(u.version.as_str())
    }

    /// 浏览器下载地址:Linux / Android 用下载直链,Windows / macOS 用第一个填了的安装包直链。
    pub fn download_url(&self, platform: &str) -> Option<Result<String, String>> {
        let u = self.active(platform)?;
        let (url, pan123_auth) = match package_spec(platform) {
            Some(_) => u
                .packages
                .iter()
                .find(|p| !p.url.is_empty())
                .map(|p| (p.url.clone(), p.pan123_auth))?,
            None => (
                u.download_url.replace("{version}", &u.version),
                u.download_pan123_auth,
            ),
        };
        (!url.is_empty()).then(|| self.direct_link(&url, pan123_auth))
    }

    /// 直链的实际访问地址:开了 123 云盘鉴权的附上 auth_key。
    pub fn direct_link(&self, url: &str, pan123_auth: bool) -> Result<String, String> {
        if pan123_auth {
            pan123::sign(url, &self.pan123_auth_key)
        } else {
            Ok(url.to_string())
        }
    }
}

/// 客户端能比较的版本号:`x.y.z` 或 `x.y.z+构建号`,x/y/z 不超过 65535。
pub fn is_valid_version(v: &str) -> bool {
    let (base, build) = v.split_once('+').unwrap_or((v, "0"));
    let number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = base.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| number(p) && p.parse::<u16>().is_ok())
        && number(build)
        && build.parse::<u32>().is_ok()
}

/// rustdesk `hbb_common::version_check_request` 请求体里用到的字段。
#[derive(Debug, Default, Deserialize)]
pub struct VersionCheckRequest {
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub arch: String,
}

/// 版本检查响应:release 页地址,客户端从中解析版本号;没有更新时为空。
#[derive(Debug, Default, Serialize)]
pub struct VersionCheckResponse {
    pub url: String,
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

/// 把客户端上报的 os 字段(或浏览器 User-Agent)映射到我们的平台键。
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
