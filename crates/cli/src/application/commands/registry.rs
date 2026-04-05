use super::{load_project, DEFAULT_REGISTRY};
use crate::application::{config, credentials, signing};
use crate::infrastructure::ui;
use nexa_compiler::compile_to_bundle;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::Duration,
};

// ── HTTP timeout constant ─────────────────────────────────────────────────────

/// Timeout for all outbound registry HTTP requests (Q5).
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Build a blocking HTTP client with sensible timeouts.
pub(super) fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .connect_timeout(HTTP_CONNECT_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

// ── Auth commands ─────────────────────────────────────────────────────────────

pub fn register(registry_override: Option<String>) {
    let registry = registry_override
        .or_else(|| Some(config::load().registry))
        .unwrap_or_else(|| DEFAULT_REGISTRY.to_string());

    ui::header("Create account");
    let email = ui::input("Email", None);
    let password = ui::password("Password");

    let sp = ui::spinner(format!("Creating account on {registry}…"));
    let url = format!("{registry}/v1/auth/register");
    match post_json(
        &url,
        &serde_json::json!({ "email": email, "password": password }),
        None,
    ) {
        Ok(body) => {
            if let Some(token) = body["token"].as_str() {
                credentials::save(&registry, token);
                ui::done(&sp, format!("Account created  ·  logged in as {email}"));
            } else {
                ui::fail(&sp, body["error"].as_str().unwrap_or("unknown error"));
            }
        }
        Err(e) => ui::fail(&sp, e),
    }
}

pub fn login(registry_override: Option<String>) {
    let registry = registry_override
        .or_else(|| Some(config::load().registry))
        .unwrap_or_else(|| DEFAULT_REGISTRY.to_string());

    ui::header("Login");
    let email = ui::input("Email", None);
    let password = ui::password("Password");

    let sp = ui::spinner(format!("Authenticating with {registry}…"));
    let url = format!("{registry}/v1/auth/login");
    match post_json(
        &url,
        &serde_json::json!({ "email": email, "password": password }),
        None,
    ) {
        Ok(body) => {
            if let Some(token) = body["token"].as_str() {
                credentials::save(&registry, token);
                ui::done(&sp, format!("Logged in as {email}"));
            } else {
                ui::fail(&sp, body["error"].as_str().unwrap_or("invalid credentials"));
            }
        }
        Err(e) => ui::fail(&sp, e),
    }
}

// ── Publish ───────────────────────────────────────────────────────────────────

pub fn publish(
    project_dir: Option<PathBuf>,
    module_override: Option<String>,
    registry_override: Option<String>,
) {
    let proj = load_project(project_dir);
    let mod_name = module_override.unwrap_or_else(|| proj.main_module_name().to_string());
    let app_name = proj.project.name.clone();
    let app_version = proj.project.version.clone();
    let bundle_name = format!("{app_name}-{mod_name}");

    let creds = credentials::load().unwrap_or_else(|| {
        ui::die("not logged in. Run `nexa login` first.");
    });
    let registry = registry_override.unwrap_or(creds.registry.clone());

    let sp = ui::spinner(format!("Packaging {bundle_name} v{app_version}…"));
    let bundle = match compile_to_bundle(
        &proj.module_entry(&mod_name),
        &proj.module_src_root(&mod_name),
        proj.root(),
        &mod_name,
        &bundle_name,
        &app_version,
    ) {
        Ok(b) => b,
        Err(e) => ui::fail(&sp, e.to_string()),
    };

    let tmp_path = std::env::temp_dir().join(format!("{app_name}-{app_version}.nexa"));
    {
        use std::io::Write as _;
        let file =
            fs::File::create(&tmp_path).unwrap_or_else(|e| ui::fail(&sp, e.to_string()));
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("app.nxb", opts)
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP: {e}")));
        zip.write_all(&bundle.nxb)
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP write nxb: {e}")));
        zip.start_file("manifest.json", opts)
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP: {e}")));
        zip.write_all(bundle.manifest.as_bytes())
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP write manifest: {e}")));
        zip.start_file("signature.sig", opts)
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP: {e}")));
        zip.write_all(bundle.signature.as_bytes())
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP write signature: {e}")));
        let src_entry = format!("src/{}", bundle.source_filename);
        zip.start_file(&src_entry, opts)
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP: {e}")));
        zip.write_all(bundle.source.as_bytes())
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP write source: {e}")));
        zip.finish()
            .unwrap_or_else(|e| ui::fail(&sp, format!("ZIP finalize: {e}")));
    }

    sp.set_message(format!(
        "Publishing {bundle_name}@{app_version} to {registry}…"
    ));

    let url = format!("{registry}/v1/packages/{bundle_name}/publish");
    let file_bytes = fs::read(&tmp_path).unwrap_or_else(|e| ui::fail(&sp, e.to_string()));
    let _ = fs::remove_file(&tmp_path);

    let signing_key = signing::load_or_generate();
    let pubkey_b64 = signing::public_key_b64(&signing_key);
    let sig_b64 = signing::sign_bundle(&signing_key, &file_bytes);

    let client = http_client();
    let part = reqwest::blocking::multipart::Part::bytes(file_bytes)
        .file_name(format!("{app_name}.nexa"))
        .mime_str("application/octet-stream")
        .unwrap_or_else(|e| ui::fail(&sp, format!("MIME type error: {e}")));
    let form = reqwest::blocking::multipart::Form::new().part("file", part);

    match client
        .post(&url)
        .bearer_auth(&creds.token)
        .header("X-Nexa-Signing-Key", &pubkey_b64)
        .header("X-Nexa-Signature", &sig_b64)
        .multipart(form)
        .send()
    {
        Ok(resp) if resp.status().is_success() => {
            ui::done(&sp, format!("Published {bundle_name}@{app_version}"));
        }
        Ok(resp) => {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            ui::fail(&sp, body["error"].as_str().unwrap_or("publish failed"));
        }
        Err(e) => ui::fail(&sp, e.to_string()),
    }
}

// ── Install ───────────────────────────────────────────────────────────────────

pub fn install(
    package_arg: Option<String>,
    project_dir: Option<PathBuf>,
    module_override: Option<String>,
) {
    let proj = load_project(project_dir);
    let registries = proj.compiler.all_registries();

    let (packages_to_install, install_for_module): (Vec<(String, String)>, Option<String>) =
        if let Some(arg) = package_arg {
            let pkg = if let Some((name, ver)) = arg.split_once('@') {
                vec![(name.to_string(), ver.to_string())]
            } else {
                vec![(arg, "latest".to_string())]
            };
            (pkg, module_override.clone())
        } else {
            let deps = if let Some(ref mod_name) = module_override {
                proj.modules
                    .get(mod_name.as_str())
                    .map(|m| &m.dependencies)
                    .cloned()
                    .unwrap_or_default()
            } else {
                proj.project.dependencies.clone()
            };
            let pkgs = deps
                .iter()
                .map(|(name, ver)| (name.clone(), ver.trim_start_matches('^').to_string()))
                .collect();
            (pkgs, module_override.clone())
        };

    if packages_to_install.is_empty() {
        ui::info("No dependencies to install.");
        return;
    }

    let libs_dir = if let Some(ref mod_name) = install_for_module {
        proj.module_lib_dir(mod_name)
    } else {
        proj.lib_dir()
    };
    fs::create_dir_all(&libs_dir)
        .unwrap_or_else(|e| ui::die(format!("cannot create nexa-libs/: {e}")));

    let mut lock = load_lockfile(&libs_dir);

    for (name, version) in &packages_to_install {
        let sp = ui::spinner(format!("Installing {name}@{version}…"));
        let bundle = try_download(&registries, name, version);
        let (registry_url, bundle_bytes) = bundle.unwrap_or_else(|| {
            ui::fail(
                &sp,
                format!("package {name}@{version} not found in any registry"),
            )
        });

        verify_bundle_signature(&bundle_bytes, name);

        let pkg_dir = libs_dir.join(format!("{name}@{version}"));
        extract_bundle_to(&bundle_bytes, &pkg_dir);

        let manifest_path = pkg_dir.join("manifest.json");
        let resolved_version = fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["version"].as_str().map(String::from))
            .unwrap_or_else(|| version.clone());

        let sig = fs::read_to_string(pkg_dir.join("signature.sig")).unwrap_or_default();
        lock.packages.retain(|p: &LockEntry| p.name != *name);
        lock.packages.push(LockEntry {
            name: name.clone(),
            version: resolved_version.clone(),
            registry: registry_url,
            signature: sig.trim().to_string(),
        });

        ui::done(&sp, format!("{name}@{resolved_version}"));
    }

    save_lockfile(&libs_dir, &lock);

    update_project_dependencies(
        proj.root(),
        install_for_module.as_deref(),
        &packages_to_install,
        &lock,
    );

    ui::blank();
    ui::success(format!(
        "{} package(s) installed.",
        packages_to_install.len()
    ));
}

// ── Search / Info ─────────────────────────────────────────────────────────────

pub fn search(query: Option<String>, registry_override: Option<String>, limit: u32) {
    let registry = registry_override
        .or_else(|| Some(config::load().registry))
        .unwrap_or_else(|| DEFAULT_REGISTRY.to_string());

    let q = query.clone().unwrap_or_default();
    let sp = ui::spinner(format!("Searching {registry}…"));

    let url = format!("{registry}/v1/packages?q={q}&per_page={limit}");
    let result = http_client()
        .get(&url)
        .send()
        .and_then(|r| r.json::<serde_json::Value>());

    sp.finish_and_clear();

    match result {
        Ok(body) => {
            let packages = body.as_array().cloned().unwrap_or_default();
            if packages.is_empty() {
                ui::blank();
                ui::info(if q.is_empty() {
                    "No packages found on the registry.".to_string()
                } else {
                    format!("No packages found for '{q}'.")
                });
                ui::blank();
                return;
            }

            ui::blank();
            if q.is_empty() {
                println!("  Packages on \x1b[1m{registry}\x1b[0m\n");
            } else {
                println!("  Results for \x1b[1m\"{q}\"\x1b[0m on {registry}\n");
            }

            let mut table = ui::Table::new(vec!["Package", "Description"]);
            for pkg in &packages {
                let name = pkg["name"].as_str().unwrap_or("?").to_string();
                let desc = pkg["description"].as_str().unwrap_or("—").to_string();
                table.row(vec![name, desc]);
            }
            table.print();

            ui::blank();
            ui::hint(format!(
                "  {} result(s)  ·  install: nexa install <name>",
                packages.len()
            ));
            ui::blank();
        }
        Err(e) => ui::die(format!("search failed: {e}")),
    }
}

pub fn info(package: String, registry_override: Option<String>) {
    let registry = registry_override
        .or_else(|| Some(config::load().registry))
        .unwrap_or_else(|| DEFAULT_REGISTRY.to_string());

    let sp = ui::spinner(format!("Fetching info for {package}…"));
    let url = format!("{registry}/v1/packages/{package}");
    let result = http_client()
        .get(&url)
        .send()
        .and_then(|r| r.json::<serde_json::Value>());

    sp.finish_and_clear();

    match result {
        Ok(body) => {
            if body.get("error").is_some() {
                ui::die(format!("package '{package}' not found on {registry}"));
            }

            ui::blank();
            println!(
                "  \x1b[1;36m{}\x1b[0m",
                body["name"].as_str().unwrap_or(&package)
            );
            ui::blank();

            let versions = body["versions"].as_array().cloned().unwrap_or_default();
            if versions.is_empty() {
                ui::info("No versions published yet.");
            } else {
                let mut table = ui::Table::new(vec!["Version", "Published"]);
                for v in &versions {
                    let ver = v["version"].as_str().unwrap_or("?").to_string();
                    let published = v["published_at"].as_str().unwrap_or("—").to_string();
                    table.row(vec![ver, published]);
                }
                table.print();
            }

            ui::blank();
            let latest = versions
                .last()
                .and_then(|v| v["version"].as_str())
                .unwrap_or("latest");
            ui::hint(format!("  Install:  nexa install {package}@{latest}"));
            ui::blank();
        }
        Err(e) => ui::die(format!("could not fetch package info: {e}")),
    }
}

// ── Shared HTTP helpers ───────────────────────────────────────────────────────

fn post_json(
    url: &str,
    body: &serde_json::Value,
    token: Option<&str>,
) -> Result<serde_json::Value, String> {
    let client = http_client();
    let mut req = client.post(url).json(body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().map_err(|e| e.to_string())?;
    resp.json::<serde_json::Value>().map_err(|e| e.to_string())
}

/// Try each registry in order; return the first successful download.
fn try_download(
    registries: &[(String, Option<String>)],
    name: &str,
    version: &str,
) -> Option<(String, Vec<u8>)> {
    let client = http_client(); // Q5: uses HTTP_TIMEOUT
    for (url, key) in registries {
        let endpoint = format!("{url}/v1/packages/{name}/{version}/download");
        let mut req = client.get(&endpoint);
        if let Some(k) = key {
            req = req.header("X-Api-Key", k);
        }
        if let Ok(resp) = req.send() {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes() {
                    return Some((url.clone(), bytes.to_vec()));
                }
            }
        }
    }
    None
}

/// Verify the SHA-256 bundle signature (Q2: no bare .unwrap()).
fn verify_bundle_signature(bundle: &[u8], name: &str) {
    let cursor = Cursor::new(bundle);
    let mut archive = zip::ZipArchive::new(cursor)
        .unwrap_or_else(|e| ui::die(format!("invalid bundle for {name}: {e}")));

    let nxb = read_zip_bytes(&mut archive, "app.nxb", name);
    let manifest_str = read_zip_string(&mut archive, "manifest.json", name);
    let sig_str = read_zip_string(&mut archive, "signature.sig", name);

    let mut hasher = Sha256::new();
    hasher.update(&nxb);
    hasher.update(manifest_str.as_bytes());
    let computed = format!("{:x}", hasher.finalize());
    if computed != sig_str.trim() {
        ui::die(format!(
            "signature verification failed for {name} — bundle may be corrupted"
        ));
    }
}

fn read_zip_bytes(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    entry_name: &str,
    pkg_name: &str,
) -> Vec<u8> {
    let mut entry = archive
        .by_name(entry_name)
        .unwrap_or_else(|_| ui::die(format!("bundle for '{pkg_name}' is missing '{entry_name}'")));
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).unwrap_or_else(|e| {
        ui::die(format!(
            "failed to read '{entry_name}' from bundle '{pkg_name}': {e}"
        ))
    });
    buf
}

fn read_zip_string(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    entry_name: &str,
    pkg_name: &str,
) -> String {
    let mut entry = archive
        .by_name(entry_name)
        .unwrap_or_else(|_| ui::die(format!("bundle for '{pkg_name}' is missing '{entry_name}'")));
    let mut buf = String::new();
    entry.read_to_string(&mut buf).unwrap_or_else(|e| {
        ui::die(format!(
            "failed to read '{entry_name}' from bundle '{pkg_name}': {e}"
        ))
    });
    buf
}

/// Extract all ZIP entries from `bundle` into `dest` (Q2: no bare .unwrap()).
fn extract_bundle_to(bundle: &[u8], dest: &Path) {
    fs::create_dir_all(dest)
        .unwrap_or_else(|e| ui::die(format!("cannot create {}: {e}", dest.display())));
    let cursor = Cursor::new(bundle);
    let mut archive = zip::ZipArchive::new(cursor)
        .unwrap_or_else(|e| ui::die(format!("invalid bundle: {e}")));
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .unwrap_or_else(|e| ui::die(format!("cannot read bundle entry {i}: {e}")));
        let out_path = dest.join(entry.name());
        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).unwrap_or_else(|e| {
            ui::die(format!("cannot read bundle entry '{}': {e}", entry.name()))
        });
        fs::write(&out_path, &buf)
            .unwrap_or_else(|e| ui::die(format!("write {}: {e}", out_path.display())));
    }
}

// ── Theme install (called from config.rs) ─────────────────────────────────────

pub(super) fn theme_install(name: String, registry_override: Option<String>) {
    let registry = registry_override
        .or_else(|| Some(config::load().registry))
        .unwrap_or_else(|| DEFAULT_REGISTRY.to_string());

    let themes_dir = config::themes_dir();
    let theme_dir = themes_dir.join(&name);

    if theme_dir.exists() {
        if !ui::confirm(
            &format!("Theme '{name}' is already installed. Reinstall?"),
            false,
        ) {
            return;
        }
        let _ = fs::remove_dir_all(&theme_dir);
    }

    let sp = ui::spinner(format!("Downloading theme {name} from {registry}…"));
    let registries = vec![(registry.clone(), None::<String>)];
    let bundle = try_download(&registries, &name, "latest");
    let (_, bundle_bytes) =
        bundle.unwrap_or_else(|| ui::fail(&sp, format!("theme '{name}' not found on {registry}")));

    fs::create_dir_all(&theme_dir)
        .unwrap_or_else(|e| ui::fail(&sp, format!("cannot create theme directory: {e}")));
    extract_bundle_to(&bundle_bytes, &theme_dir);

    ui::done(
        &sp,
        format!("Theme '{name}' installed  →  activate with: nexa config set theme {name}"),
    );
}

// ── Package lockfile ──────────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Lockfile {
    packages: Vec<LockEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct LockEntry {
    name: String,
    version: String,
    registry: String,
    signature: String,
}

fn load_lockfile(libs_dir: &Path) -> Lockfile {
    fs::read_to_string(libs_dir.join(".lock"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_lockfile(libs_dir: &Path, lock: &Lockfile) {
    let json = serde_json::to_string_pretty(lock).unwrap_or_else(|e| {
        ui::die(format!("could not serialize lockfile: {e}"));
    });
    fs::write(libs_dir.join(".lock"), json).unwrap_or_else(|e| {
        ui::warn(format!("could not write lockfile: {e}"));
    });
}

fn update_project_dependencies(
    project_root: &Path,
    module_name: Option<&str>,
    installed: &[(String, String)],
    lock: &Lockfile,
) {
    let path = if let Some(mod_name) = module_name {
        project_root
            .join("modules")
            .join(mod_name)
            .join("module.json")
    } else {
        project_root.join("project.json")
    };

    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return,
    };
    let mut value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return,
    };

    if let Some(obj) = value.as_object_mut() {
        let deps = obj
            .entry("dependencies")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(deps_map) = deps.as_object_mut() {
            for (name, _) in installed {
                let pinned = lock
                    .packages
                    .iter()
                    .find(|e| &e.name == name)
                    .map(|e| e.version.as_str())
                    .unwrap_or("latest");
                deps_map.insert(name.clone(), serde_json::Value::String(pinned.to_string()));
            }
        }
    }

    if let Ok(updated) = serde_json::to_string_pretty(&value) {
        let _ = fs::write(&path, updated);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Serialise tests that mutate env vars so they don't race.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // ── Fixtures ──────────────────────────────────────────────────────────────

    const MINIMAL_APP: &str = r#"app TestApp {
    server { port: 3000; }
    public window HomePage {
        public render() => Component {
            return Page { Text("Hello") };
        }
    }
    route "/" => HomePage;
}"#;

    /// Build a minimal valid `.nexa` ZIP bundle in memory.
    ///
    /// The bundle contains `app.nxb`, `manifest.json`, `signature.sig`, and
    /// `src/app.nx`.  The signature is a real SHA-256 over (nxb || manifest)
    /// so that `verify_bundle_signature` passes.
    fn build_test_bundle(name: &str, version: &str) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        use std::io::Write as _;

        let nxb_magic = b"NXB\x01\x00\x00\x00\x00";
        let manifest = format!(
            r#"{{"name":"{name}","version":"{version}","nexa_version":"0.1.0","nxb_version":1,"created_at":0}}"#
        );
        let mut hasher = Sha256::new();
        hasher.update(nxb_magic);
        hasher.update(manifest.as_bytes());
        let signature = format!("{:x}", hasher.finalize());

        let mut buf = Vec::new();
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("app.nxb", opts).unwrap();
        zip.write_all(nxb_magic).unwrap();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.start_file("signature.sig", opts).unwrap();
        zip.write_all(signature.as_bytes()).unwrap();
        zip.start_file("src/app.nx", opts).unwrap();
        zip.write_all(b"// source\n").unwrap();
        zip.finish().unwrap();
        buf
    }

    /// Create a minimal valid project layout in `dir`.
    fn make_project(dir: &std::path::Path, registry_url: &str) {
        fs::write(
            dir.join("project.json"),
            r#"{"name":"test-app","version":"0.1.0","author":"Test","modules":["core"]}"#,
        )
        .unwrap();
        let yaml = format!(
            "version: \"0.1\"\nmain_module: \"core\"\nregistry: \"{registry_url}\"\n"
        );
        fs::write(dir.join("nexa-compiler.yaml"), yaml).unwrap();

        let src_main = dir.join("modules").join("core").join("src").join("main");
        fs::create_dir_all(&src_main).unwrap();
        fs::write(
            dir.join("modules").join("core").join("module.json"),
            r#"{"name":"core","main":"app.nx"}"#,
        )
        .unwrap();
        fs::write(src_main.join("app.nx"), MINIMAL_APP).unwrap();
    }

    // ── install tests ─────────────────────────────────────────────────────────

    #[test]
    fn install_downloads_and_extracts_package() {
        let mut server = mockito::Server::new();
        let bundle = build_test_bundle("test-pkg", "1.0.0");

        let _m = server
            .mock("GET", "/v1/packages/test-pkg/1.0.0/download")
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(bundle)
            .create();

        let tmp = TempDir::new().unwrap();
        make_project(tmp.path(), &server.url());

        install(
            Some("test-pkg@1.0.0".to_string()),
            Some(tmp.path().to_path_buf()),
            None,
        );

        let pkg_dir = tmp.path().join("lib").join("test-pkg@1.0.0");
        assert!(pkg_dir.exists(), "package directory not created");
        assert!(pkg_dir.join("manifest.json").exists(), "manifest.json missing");
        assert!(pkg_dir.join("app.nxb").exists(), "app.nxb missing");
        assert!(pkg_dir.join("signature.sig").exists(), "signature.sig missing");
    }

    #[test]
    fn install_creates_lockfile_entry() {
        let mut server = mockito::Server::new();
        let bundle = build_test_bundle("my-lib", "2.3.1");

        let _m = server
            .mock("GET", "/v1/packages/my-lib/2.3.1/download")
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(bundle)
            .create();

        let tmp = TempDir::new().unwrap();
        make_project(tmp.path(), &server.url());

        install(
            Some("my-lib@2.3.1".to_string()),
            Some(tmp.path().to_path_buf()),
            None,
        );

        let lock_path = tmp.path().join("lib").join(".lock");
        assert!(lock_path.exists(), ".lock file not created");
        let lock: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
        let packages = lock["packages"].as_array().unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0]["name"].as_str().unwrap(), "my-lib");
        assert_eq!(packages[0]["version"].as_str().unwrap(), "2.3.1");
    }

    #[test]
    fn install_updates_project_json_dependencies() {
        let mut server = mockito::Server::new();
        let bundle = build_test_bundle("ui-kit", "0.5.0");

        let _m = server
            .mock("GET", "/v1/packages/ui-kit/0.5.0/download")
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(bundle)
            .create();

        let tmp = TempDir::new().unwrap();
        make_project(tmp.path(), &server.url());

        install(
            Some("ui-kit@0.5.0".to_string()),
            Some(tmp.path().to_path_buf()),
            None,
        );

        let pj: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(tmp.path().join("project.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            pj["dependencies"]["ui-kit"].as_str().unwrap(),
            "0.5.0",
            "project.json dependencies not updated"
        );
    }

    #[test]
    fn install_from_project_dependencies() {
        let mut server = mockito::Server::new();
        let bundle = build_test_bundle("core-utils", "1.0.0");

        let _m = server
            .mock("GET", "/v1/packages/core-utils/1.0.0/download")
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(bundle)
            .create();

        let tmp = TempDir::new().unwrap();
        make_project(tmp.path(), &server.url());

        // Write a dependency directly into project.json.
        let pj_path = tmp.path().join("project.json");
        let mut pj: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&pj_path).unwrap()).unwrap();
        pj["dependencies"]["core-utils"] = serde_json::json!("1.0.0");
        fs::write(&pj_path, serde_json::to_string_pretty(&pj).unwrap()).unwrap();

        // install with no explicit package — should read from project.json.
        install(None, Some(tmp.path().to_path_buf()), None);

        let pkg_dir = tmp.path().join("lib").join("core-utils@1.0.0");
        assert!(pkg_dir.exists(), "core-utils not installed from project dependencies");
    }

    #[test]
    fn install_into_module_lib_dir() {
        let mut server = mockito::Server::new();
        let bundle = build_test_bundle("form-lib", "1.0.0");

        let _m = server
            .mock("GET", "/v1/packages/form-lib/1.0.0/download")
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(bundle)
            .create();

        let tmp = TempDir::new().unwrap();
        make_project(tmp.path(), &server.url());

        // Add a second module.
        let api_src = tmp.path().join("modules").join("api").join("src").join("main");
        fs::create_dir_all(&api_src).unwrap();
        fs::write(
            tmp.path().join("modules").join("api").join("module.json"),
            r#"{"name":"api","main":"app.nx"}"#,
        )
        .unwrap();
        fs::write(api_src.join("app.nx"), "").unwrap();
        // Update project.json to include the api module.
        let mut pj: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(tmp.path().join("project.json")).unwrap(),
        )
        .unwrap();
        pj["modules"] = serde_json::json!(["core", "api"]);
        fs::write(
            tmp.path().join("project.json"),
            serde_json::to_string_pretty(&pj).unwrap(),
        )
        .unwrap();

        install(
            Some("form-lib@1.0.0".to_string()),
            Some(tmp.path().to_path_buf()),
            Some("api".to_string()),
        );

        let pkg_dir = tmp
            .path()
            .join("modules")
            .join("api")
            .join("lib")
            .join("form-lib@1.0.0");
        assert!(pkg_dir.exists(), "form-lib not installed in api module lib dir");
    }

    // ── publish tests ─────────────────────────────────────────────────────────

    #[test]
    fn publish_posts_bundle_to_registry() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        let mut server = mockito::Server::new();
        let _m = server
            .mock("POST", "/v1/packages/test-app-core/publish")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"pkg-123","status":"ok"}"#)
            .create();

        let tmp = TempDir::new().unwrap();
        make_project(tmp.path(), &server.url());

        // Provide credentials via env var (CI-friendly path in credentials::load).
        std::env::set_var("NEXA_TOKEN", "test-bearer-token");
        std::env::set_var("NEXA_REGISTRY", &server.url());

        publish(
            Some(tmp.path().to_path_buf()),
            None,
            Some(server.url()),
        );

        std::env::remove_var("NEXA_TOKEN");
        std::env::remove_var("NEXA_REGISTRY");

        _m.assert();
    }

    #[test]
    fn publish_posts_to_override_registry() {
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        let mut server = mockito::Server::new();
        let _m = server
            .mock("POST", "/v1/packages/test-app-core/publish")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"pkg-456","status":"ok"}"#)
            .create();

        let tmp = TempDir::new().unwrap();
        // Project uses a different default registry — the override should win.
        make_project(tmp.path(), "https://registry.example.com");

        std::env::set_var("NEXA_TOKEN", "test-bearer-token");
        std::env::set_var("NEXA_REGISTRY", &server.url());

        publish(
            Some(tmp.path().to_path_buf()),
            None,
            Some(server.url()),
        );

        std::env::remove_var("NEXA_TOKEN");
        std::env::remove_var("NEXA_REGISTRY");

        _m.assert();
    }

    // ── Internal helper tests ─────────────────────────────────────────────────

    #[test]
    fn verify_bundle_signature_accepts_valid_bundle() {
        let bundle = build_test_bundle("my-pkg", "1.0.0");
        // Should not panic or call process::exit.
        verify_bundle_signature(&bundle, "my-pkg");
    }

    #[test]
    fn extract_bundle_to_creates_all_files() {
        let bundle = build_test_bundle("extract-test", "1.0.0");
        let tmp = TempDir::new().unwrap();
        extract_bundle_to(&bundle, tmp.path());
        assert!(tmp.path().join("manifest.json").exists());
        assert!(tmp.path().join("app.nxb").exists());
        assert!(tmp.path().join("signature.sig").exists());
        assert!(tmp.path().join("src").join("app.nx").exists());
    }
}
