use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Global CLI configuration stored in `~/.nexa/config.json`.
/// All fields have sensible defaults — a missing or empty file is valid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexaConfig {
    /// Default registry URL used by install / publish / search.
    #[serde(default = "default_registry")]
    pub registry: String,

    /// Update channel: stable | snapshot | latest.
    #[serde(default = "default_channel")]
    pub channel: String,

    /// Active CLI theme name (matches a directory in `~/.nexa/themes/`).
    #[serde(default)]
    pub theme: Option<String>,

    /// Whether passive background update checks are enabled.
    #[serde(default = "default_true")]
    pub update_check: bool,
}

fn default_registry() -> String {
    "https://registry.nexa-lang.org".to_string()
}
fn default_channel() -> String {
    "stable".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for NexaConfig {
    fn default() -> Self {
        Self {
            registry: default_registry(),
            channel: default_channel(),
            theme: None,
            update_check: true,
        }
    }
}

// ── Paths ─────────────────────────────────────────────────────────────────────

pub fn nexa_home() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".nexa")
}

pub fn config_path() -> PathBuf {
    nexa_home().join("config.json")
}

pub fn themes_dir() -> PathBuf {
    nexa_home().join("themes")
}

// ── Load / save ───────────────────────────────────────────────────────────────

pub fn load() -> NexaConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(config: &NexaConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, json);
    }
}

// ── get / set API ─────────────────────────────────────────────────────────────

pub const KEYS: &[&str] = &["registry", "channel", "theme", "update_check"];

pub fn get(key: &str) -> Option<String> {
    let c = load();
    match key {
        "registry" => Some(c.registry),
        "channel" => Some(c.channel),
        "theme" => Some(c.theme.unwrap_or_else(|| "default".to_string())),
        "update_check" => Some(c.update_check.to_string()),
        _ => None,
    }
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    let mut c = load();
    match key {
        "registry" => c.registry = value.to_string(),
        "channel" => {
            if !["stable", "snapshot", "latest"].contains(&value) {
                return Err("channel must be one of: stable, snapshot, latest".to_string());
            }
            c.channel = value.to_string();
        }
        "theme" => {
            c.theme = if value == "default" {
                None
            } else {
                Some(value.to_string())
            }
        }
        "update_check" => {
            c.update_check = value
                .parse::<bool>()
                .map_err(|_| "update_check must be true or false".to_string())?;
        }
        _ => {
            return Err(format!(
                "unknown key '{key}'. Available: {}",
                KEYS.join(", ")
            ))
        }
    }
    save(&c);
    Ok(())
}

// ── Theme helpers ─────────────────────────────────────────────────────────────

/// Returns the names of all installed themes.
pub fn list_themes() -> Vec<String> {
    let dir = themes_dir();
    std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| {
                let e = e.ok()?;
                if e.file_type().ok()?.is_dir() {
                    Some(e.file_name().to_string_lossy().into_owned())
                } else {
                    None
                }
            })
            .collect()
        })
        .unwrap_or_default()
}

/// Returns the active theme name.
pub fn active_theme() -> String {
    load().theme.unwrap_or_else(|| "default".to_string())
}
