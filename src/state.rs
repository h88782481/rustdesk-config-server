use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;

use crate::config::AppConfig;
use crate::models::Store;

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

pub struct Inner {
    pub cfg: AppConfig,
    pub data: RwLock<Store>,
    pub sessions: RwLock<HashSet<String>>,
    pub data_path: PathBuf,
    /// 更新清单签名私钥,客户端只信任编译进去的对应公钥。
    pub signing_key: SigningKey,
}

impl AppState {
    pub fn load(cfg: AppConfig) -> Result<Self, String> {
        let dir = PathBuf::from(&cfg.data_dir);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create data dir {}: {e}", dir.display()))?;
        let data_path = dir.join("config.json");
        let mut store = match std::fs::read(&data_path) {
            Ok(bytes) => serde_json::from_slice::<Store>(&bytes).unwrap_or_default(),
            Err(_) => Store::default(),
        };
        store.ensure_platforms();
        let signing_key = load_signing_key(&dir.join("update_ed25519.key"))?;

        let state = AppState {
            inner: Arc::new(Inner {
                cfg,
                data: RwLock::new(store),
                sessions: RwLock::new(HashSet::new()),
                data_path,
                signing_key,
            }),
        };
        state
            .persist()
            .map_err(|e| format!("failed to write initial config: {e}"))?;
        Ok(state)
    }

    pub fn read_store(&self) -> Store {
        self.inner.data.read().unwrap().clone()
    }

    pub fn mutate<F: FnOnce(&mut Store)>(&self, f: F) -> std::io::Result<()> {
        {
            let mut g = self.inner.data.write().unwrap();
            f(&mut g);
        }
        self.persist()
    }

    /// 更新公钥(base64),填到 rdgen 的 update_public_key。
    pub fn update_public_key(&self) -> String {
        STANDARD.encode(self.inner.signing_key.verifying_key().as_bytes())
    }

    /// 原子写:先写临时文件再 rename,避免写一半损坏。
    fn persist(&self) -> std::io::Result<()> {
        let json = {
            let g = self.inner.data.read().unwrap();
            serde_json::to_string_pretty(&*g)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
        };
        let tmp = self.inner.data_path.with_extension("json.tmp");
        std::fs::write(&tmp, json.as_bytes())?;
        std::fs::rename(&tmp, &self.inner.data_path)?;
        Ok(())
    }
}

/// 读取私钥(base64 的 32 字节种子);首次启动时生成,仅属主可读。
fn load_signing_key(path: &Path) -> Result<SigningKey, String> {
    let seed = match std::fs::read_to_string(path) {
        Ok(text) => STANDARD
            .decode(text.trim())
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            .ok_or_else(|| format!("invalid update signing key {}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut seed = [0u8; 32];
            getrandom::getrandom(&mut seed)
                .map_err(|e| format!("failed to generate update signing key: {e}"))?;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            opts.open(path)
                .and_then(|mut f| f.write_all(STANDARD.encode(seed).as_bytes()))
                .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            tracing::info!("generated update signing key {}", path.display());
            seed
        }
        Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
    };
    Ok(SigningKey::from_bytes(&seed))
}
