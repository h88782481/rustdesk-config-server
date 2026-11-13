//! 123 云盘直链鉴权:直链后附加
//! `auth_key=<过期时间>-<随机数>-<UID>-md5(<路径>-<过期时间>-<随机数>-<UID>-<密钥>)`,
//! 路径是解码后的 URL 路径,第一段就是 UID。

use md5::{Digest, Md5};
use percent_encoding::percent_decode_str;
use reqwest::Url;

use crate::models::now_unix;

/// 签名有效期,与官方示例一致。123 云盘只在请求开始时校验,下载耗时不受限制。
const TTL_SECS: u64 = 15 * 60;

/// 给直链附上 auth_key。
pub fn sign(url: &str, key: &str) -> Result<String, String> {
    if key.is_empty() {
        return Err("未设置 123 云盘鉴权密钥".to_string());
    }
    let mut url = Url::parse(url).map_err(|e| format!("直链 {url} 无效: {e}"))?;
    let path = percent_decode_str(url.path())
        .decode_utf8()
        .map_err(|e| format!("直链 {url} 无效: {e}"))?
        .into_owned();
    let uid = path
        .split('/')
        .nth(1)
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| format!("直链 {url} 不是 123 云盘直链,路径应以 /UID/ 开头"))?;
    let mut rand = [0u8; 4];
    getrandom::getrandom(&mut rand).map_err(|e| e.to_string())?;
    let rand = u32::from_le_bytes(rand);
    let expires = now_unix() + TTL_SECS;
    let hash = Md5::digest(format!("{path}-{expires}-{rand}-{uid}-{key}"));
    url.query_pairs_mut()
        .append_pair("auth_key", &format!("{expires}-{rand}-{uid}-{hash:x}"));
    Ok(url.into())
}
