use super::registry::http_client;
use crate::application::credentials;
use crate::infrastructure::ui;

pub fn token_create(name: String, registry_override: Option<String>) {
    let creds = credentials::load().unwrap_or_else(|| {
        ui::die("not logged in. Run `nexa login` first.");
    });
    let registry = registry_override.unwrap_or(creds.registry.clone());

    let sp = ui::spinner(format!("Creating token '{name}' on {registry}…"));
    let url = format!("{registry}/v1/auth/tokens");
    let result = http_client()
        .post(&url)
        .bearer_auth(&creds.token)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .and_then(|r| r.json::<serde_json::Value>());

    match result {
        Ok(body) if body.get("token").is_some() => {
            ui::done(&sp, format!("Token created  ·  name: {name}"));
            ui::blank();
            println!(
                "  \x1b[1;33mtoken:\x1b[0m  {}",
                body["token"].as_str().unwrap_or("?")
            );
            println!(
                "  \x1b[2mid:    {}\x1b[0m",
                body["id"].as_str().unwrap_or("?")
            );
            ui::blank();
            ui::warn("Save this token now — it will not be shown again.");
            ui::blank();
        }
        Ok(body) => {
            ui::fail(&sp, body["error"].as_str().unwrap_or("unknown error"));
        }
        Err(e) => ui::fail(&sp, e.to_string()),
    }
}

pub fn token_list(registry_override: Option<String>) {
    let creds = credentials::load().unwrap_or_else(|| {
        ui::die("not logged in. Run `nexa login` first.");
    });
    let registry = registry_override.unwrap_or(creds.registry.clone());

    let sp = ui::spinner(format!("Fetching tokens from {registry}…"));
    let url = format!("{registry}/v1/auth/tokens");
    let result = http_client()
        .get(&url)
        .bearer_auth(&creds.token)
        .send()
        .and_then(|r| r.json::<serde_json::Value>());

    sp.finish_and_clear();

    match result {
        Ok(body) => {
            let tokens = body.as_array().cloned().unwrap_or_default();
            if tokens.is_empty() {
                ui::info("No API tokens yet. Create one with: nexa token create <name>");
            } else {
                ui::header("API tokens");
                let mut table = ui::Table::new(vec!["ID", "Name", "Created", "Last used"]);
                for t in &tokens {
                    let id = t["id"].as_str().unwrap_or("?");
                    let name = t["name"].as_str().unwrap_or("?");
                    let created = t["created_at"].as_str().unwrap_or("?");
                    let last_used = t["last_used_at"].as_str().unwrap_or("never");
                    table.row(vec![
                        id.to_string(),
                        name.to_string(),
                        created.to_string(),
                        last_used.to_string(),
                    ]);
                }
                table.print();
                ui::blank();
                ui::hint("  Revoke:  nexa token revoke <id>");
                ui::blank();
            }
        }
        Err(e) => ui::die(format!("could not fetch tokens: {e}")),
    }
}

pub fn token_revoke(id: String, registry_override: Option<String>) {
    let creds = credentials::load().unwrap_or_else(|| {
        ui::die("not logged in. Run `nexa login` first.");
    });
    let registry = registry_override.unwrap_or(creds.registry.clone());

    if !ui::confirm(&format!("Revoke token {id}?"), false) {
        return;
    }

    let sp = ui::spinner(format!("Revoking token {id}…"));
    let url = format!("{registry}/v1/auth/tokens/{id}");
    match http_client().delete(&url).bearer_auth(&creds.token).send() {
        Ok(resp) if resp.status() == 204 => {
            ui::done(&sp, format!("Token {id} revoked."));
        }
        Ok(resp) if resp.status() == 404 => {
            ui::fail(&sp, "token not found");
        }
        Ok(resp) => {
            let status = resp.status();
            let body: serde_json::Value = resp.json().unwrap_or_default();
            ui::fail(
                &sp,
                body["error"].as_str().unwrap_or(&format!("HTTP {status}")),
            );
        }
        Err(e) => ui::fail(&sp, e.to_string()),
    }
}
