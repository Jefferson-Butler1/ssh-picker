use anyhow::{Context, Result};
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: Theme,
    pub show_user: bool,
    pub show_hostname: bool,
    pub page_size: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { theme: Theme::default(), show_user: true, show_hostname: true, page_size: 10 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Theme {
    pub accent: String,
}

impl Default for Theme {
    fn default() -> Self { Self { accent: "yellow".to_string() } }
}

pub fn load_or_default() -> Result<AppSettings> {
    let path = config_path();
    if let Ok(bytes) = fs::read(&path) {
        let cfg: AppSettings = toml::from_str(std::str::from_utf8(&bytes).unwrap_or("")).context("parse config")?;
        Ok(cfg)
    } else {
        // write default to help user customize
        let cfg = AppSettings::default();
        if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
        let _ = fs::write(&path, toml::to_string_pretty(&cfg)?);
        Ok(cfg)
    }
}

pub fn config_path() -> PathBuf {
    config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("ssh-picker")
        .join("config.toml")
}


