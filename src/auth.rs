use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::state::AppState;

/// 定长比较,避免时序侧信道泄漏 token/密码。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn get_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

/// 客户端接口鉴权:`Authorization: Bearer <CLIENT_TOKEN>`。
pub async fn require_client_token(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.strip_prefix("Bearer ").unwrap_or(v).trim().to_string());
    match provided {
        Some(t) if constant_time_eq(t.as_bytes(), st.inner.cfg.client_token.as_bytes()) => {
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// 面板接口鉴权:校验登录会话 cookie。
pub async fn require_session(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ok = get_cookie(req.headers(), "sid")
        .map(|sid| st.inner.sessions.read().unwrap().contains(&sid))
        .unwrap_or(false);
    if ok {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
