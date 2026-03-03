use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub theme: String,
    pub tab_size: usize,
    pub line_numbers: bool,
    pub tree_width: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "base16-ocean.dark".to_string(),
            tab_size: 4,
            line_numbers: true,
            tree_width: 30,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("macro")
        .join("config.toml")
}
