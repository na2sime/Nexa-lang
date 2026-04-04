use crate::application::{config, credentials, updater};
use crate::infrastructure::ui;

pub fn doctor() {
    ui::header("Nexa environment check");

    let registry = config::load().registry;
    let sp = ui::spinner(format!("Checking registry ({registry})…"));
    let ok = super::registry::http_client()
        .get(format!("{registry}/health"))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    if ok {
        ui::done(&sp, format!("Registry reachable  →  {registry}"));
    } else {
        sp.finish_and_clear();
        ui::warn(format!("Registry unreachable: {registry}"));
    }

    match credentials::load() {
        Some(c) => ui::success(format!("Logged in  →  {}", c.registry)),
        None => ui::warn("Not logged in  ·  run: nexa login"),
    }

    ui::success(format!("Config  →  {}", config::config_path().display()));

    let themes_count = config::list_themes().len();
    ui::success(format!(
        "Themes dir  →  {} installed  ({})",
        themes_count,
        config::themes_dir().display()
    ));

    ui::blank();
}

pub fn update(channel_override: Option<String>) {
    updater::run_update_command(channel_override);
}
