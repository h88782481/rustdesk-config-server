use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

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

        let state = AppState {
            inner: Arc::new(Inner {
                cfg,
                data: RwLock::new(store),
                sessions: RwLock::new(HashSet::new()),
                data_path,
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
