/// 运行期配置,全部来自环境变量。
pub struct AppConfig {
    pub bind_addr: String,
    pub client_token: String,
    pub admin_password: String,
    pub data_dir: String,
    pub cookie_secure: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let bind_addr =
            std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:21120".to_string());
        let client_token = require_env("CLIENT_TOKEN")?;
        let admin_password = require_env("ADMIN_PASSWORD")?;
        let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "/data".to_string());
        let cookie_secure = std::env::var("COOKIE_SECURE")
            .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "y"))
            .unwrap_or(false);
        Ok(Self {
            bind_addr,
            client_token,
            admin_password,
            data_dir,
            cookie_secure,
        })
    }
}

fn require_env(key: &str) -> Result<String, String> {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(format!(
            "environment variable {key} is required and must not be empty"
        )),
    }
}
