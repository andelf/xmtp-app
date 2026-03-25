use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub profile: String,
    pub xmtp_env: String,
    pub data_dir: String,
    pub ipc_socket_path: String,
    pub log_level: String,
    pub api_url: Option<String>,
}

impl AppConfig {
    pub fn for_data_dir(data_dir: &Path) -> Self {
        Self {
            schema_version: 1,
            profile: "default".to_owned(),
            xmtp_env: "dev".to_owned(),
            data_dir: data_dir.display().to_string(),
            ipc_socket_path: data_dir.join("daemon.sock").display().to_string(),
            log_level: "info".to_owned(),
            api_url: None,
        }
    }
}

pub fn save_config(path: &Path, config: &AppConfig) -> anyhow::Result<()> {
    let parent = path.parent().context("config path has no parent")?;
    fs::create_dir_all(parent).context("create config directory")?;
    let json = serde_json::to_string_pretty(config).context("serialize config")?;
    fs::write(path, json).context("write config")?;
    Ok(())
}

pub fn load_config(path: &Path) -> anyhow::Result<AppConfig> {
    let json = fs::read_to_string(path).context("read config")?;
    let config = serde_json::from_str(&json).context("deserialize config")?;
    Ok(config)
}
