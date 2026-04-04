use crate::application::config;
use crate::infrastructure::ui;

pub fn config_list() {
    ui::header("Nexa CLI configuration");
    for key in config::KEYS {
        let val = config::get(key).unwrap_or_default();
        ui::kv(key, val);
    }
    ui::blank();
    ui::hint(format!(
        "  Config file: {}",
        config::config_path().display()
    ));
    ui::blank();
}

pub fn config_get(key: String) {
    match config::get(&key) {
        Some(val) => println!("{val}"),
        None => ui::die(format!(
            "unknown key '{key}'. Available: {}",
            config::KEYS.join(", ")
        )),
    }
}

pub fn config_set(key: String, value: String) {
    match config::set(&key, &value) {
        Ok(()) => ui::success(format!("{key}  =  {value}")),
        Err(e) => ui::die(e),
    }
}

pub fn theme_list() {
    let active = config::active_theme();
    let installed = config::list_themes();

    ui::header("Installed themes");

    if installed.is_empty() {
        ui::info("No themes installed.");
        ui::blank();
        ui::hint("  Install a theme:  nexa theme add <name>");
    } else {
        for theme in &installed {
            if theme == &active {
                println!("  \x1b[1;32m●\x1b[0m  \x1b[1m{theme}\x1b[0m  \x1b[2m(active)\x1b[0m");
            } else {
                println!("  \x1b[2m○\x1b[0m  {theme}");
            }
        }
        ui::blank();
        ui::hint("  Activate:  nexa config set theme <name>");
    }
    ui::blank();
}

pub fn theme_add(name: String, registry_override: Option<String>) {
    // Themes are packages — delegate to registry download helpers.
    super::registry::theme_install(name, registry_override);
}

pub fn theme_remove(name: String) {
    use std::fs;
    let theme_dir = config::themes_dir().join(&name);
    if !theme_dir.exists() {
        ui::die(format!("theme '{name}' is not installed."));
    }

    if !ui::confirm(&format!("Remove theme '{name}'?"), true) {
        return;
    }

    fs::remove_dir_all(&theme_dir)
        .unwrap_or_else(|e| ui::die(format!("could not remove theme: {e}")));

    if config::active_theme() == name {
        let _ = config::set("theme", "default");
    }

    ui::success(format!("Theme '{name}' removed."));
}
