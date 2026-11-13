//! rdgen `allow_custom_update.py` 约定的应用内更新协议。
//!
//! 客户端从版本检查返回的 `.../releases/tag/v<版本>` 解析版本号,再到
//! `<Config Server>/rustdesk/rustdesk/releases/download/v<版本>/` 下取签名清单,
//! 用编译进去的 `custom-update` 公钥验签,按清单里的地址、大小和 sha256 下载安装包。

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::models::{package_spec, Package, Store, UpdateEntry};

pub const METADATA: &str = "rustdesk-update.json";
pub const SIGNATURE: &str = "rustdesk-update.json.sig";
const KEY_ID: &str = "custom-update";
const SIGNATURE_CONTEXT: &[u8] = b"RustDesk update metadata v1\n";

/// 版本 `version` 下所有已同步的安装包,附带安装包格式。
fn synced_packages<'a>(
    store: &'a Store,
    version: &str,
) -> Vec<(&'a UpdateEntry, &'a Package, &'static str)> {
    store
        .updates
        .iter()
        .filter(|u| u.enabled && u.version == version)
        .filter_map(|u| Some((u, package_spec(&u.platform)?.0)))
        .flat_map(|(u, format)| {
            u.packages
                .iter()
                .filter(|p| p.is_synced())
                .map(move |p| (u, p, format))
        })
        .collect()
}

/// 清单里的安装包文件名,客户端只接受 `[A-Za-z0-9._-]`。
fn file_name(package: &Package, format: &str) -> String {
    format!("rustdesk-{}.{format}", package.arch)
}

/// 清单原文,签名针对的就是这些字节;该版本没有已同步的安装包时为 None。
/// `origin` 必须与客户端编译进去的 Config Server 地址一致,否则客户端拒收安装包地址。
pub fn metadata(store: &Store, origin: &str, version: &str) -> Option<Vec<u8>> {
    let release_id = format!("v{version}");
    let packages = synced_packages(store, version);
    let published_at = packages.iter().map(|(u, ..)| u.updated_at).max()?;
    let artifacts: Vec<_> = packages
        .iter()
        .map(|(u, p, format)| {
            let file_name = file_name(p, format);
            json!({
                "platform": u.platform,
                "arch": p.arch,
                "format": format,
                "url": format!("{origin}/rustdesk/rustdesk/releases/download/{release_id}/{file_name}"),
                "file_name": file_name,
                "size": p.size,
                "sha256": p.sha256,
            })
        })
        .collect();
    let metadata = json!({
        "schema_version": 1,
        "app": "rustdesk",
        "package_id": "rustdesk",
        "version": version,
        "release_id": release_id,
        "published_at": rfc3339(published_at),
        "signature_key_id": KEY_ID,
        "artifacts": artifacts,
    });
    Some(metadata.to_string().into_bytes())
}

/// 清单签名:对「签名上下文 + 清单原文」做 ed25519 签名。
pub fn signature(key: &SigningKey, metadata: &[u8]) -> Vec<u8> {
    let signature = key.sign(&[SIGNATURE_CONTEXT, metadata].concat());
    json!({
        "schema_version": 1,
        "algorithm": "ed25519",
        "key_id": KEY_ID,
        "signature": STANDARD.encode(signature.to_bytes()),
    })
    .to_string()
    .into_bytes()
}

/// 清单里文件名为 `name` 的安装包的实际下载地址。
pub fn package_url(store: &Store, version: &str, name: &str) -> Option<Result<String, String>> {
    synced_packages(store, version)
        .into_iter()
        .find(|(_, p, format)| file_name(p, format) == name)
        .map(|(_, p, _)| store.direct_link(&p.url, p.pan123_auth))
}

/// 流式下载直链,算出大小与 sha256;数据只经过内存,不落盘。
pub async fn digest(url: &str) -> reqwest::Result<(u64, String)> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30 * 60))
        .build()?;
    let mut resp = client.get(url).send().await?.error_for_status()?;
    let (mut size, mut hasher) = (0u64, Sha256::new());
    while let Some(chunk) = resp.chunk().await? {
        size += chunk.len() as u64;
        hasher.update(&chunk);
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}

/// Unix 秒转 RFC 3339(UTC)。
fn rfc3339(secs: u64) -> String {
    // 公历换算(civil_from_days):以 0000-03-01 为纪元起点,每 400 年一个周期。
    let z = secs / 86_400 + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = era * 400 + yoe + u64::from(month <= 2);
    let t = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        t / 3_600,
        t % 3_600 / 60,
        t % 60
    )
}
