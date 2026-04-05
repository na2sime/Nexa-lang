//! End-to-end tests for the `nexa` CLI.
//!
//! These tests invoke the compiled `nexa` binary through `std::process::Command`
//! and verify the side-effects on the filesystem.  They exercise the complete
//! `init → build → package` pipeline.
//!
//! `nexa publish` is covered at the unit-test level (see
//! `crates/cli/src/application/commands/registry.rs`) because the blocking HTTP
//! client it uses cannot be invoked through `#[tokio::main]` without a
//! `spawn_blocking` wrapper.
//!
//! Run with:
//!   cargo test -p nexa --test e2e

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Path to the compiled `nexa` binary (injected by Cargo at compile time).
fn nexa_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nexa"))
}

/// Run `nexa <args>` in `dir` and return the output.
fn nexa(dir: &Path, args: &[&str]) -> Output {
    Command::new(nexa_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run nexa binary")
}

/// Run `nexa <args>` and assert the command succeeds (exit code 0).
fn nexa_ok(dir: &Path, args: &[&str]) -> Output {
    let out = nexa(dir, args);
    if !out.status.success() {
        panic!(
            "nexa {:?} failed (exit {:?}):\nstdout: {}\nstderr: {}",
            args,
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
    out
}

// ── nexa init ────────────────────────────────────────────────────────────────

#[test]
fn init_creates_project_structure() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "my-project"]);

    let proj_dir = tmp.path().join("my-project");
    assert!(proj_dir.join("project.json").exists(), "project.json missing");
    assert!(
        proj_dir.join("nexa-compiler.yaml").exists(),
        "nexa-compiler.yaml missing"
    );
    assert!(proj_dir.join(".gitignore").exists(), ".gitignore missing");
    assert!(
        proj_dir
            .join("modules")
            .join("core")
            .join("module.json")
            .exists(),
        "modules/core/module.json missing"
    );
    assert!(
        proj_dir
            .join("modules")
            .join("core")
            .join("src")
            .join("main")
            .join("app.nx")
            .exists(),
        "modules/core/src/main/app.nx missing"
    );
}

#[test]
fn init_project_json_contains_correct_name() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "hello-world"]);

    let pj: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("hello-world").join("project.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(pj["name"].as_str().unwrap(), "hello-world");
    assert_eq!(pj["version"].as_str().unwrap(), "0.1.0");
    assert!(
        pj["modules"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("core")),
        "project.json must list 'core' module"
    );
}

#[test]
fn init_module_json_has_main_field() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "test-proj"]);

    let mj: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            tmp.path()
                .join("test-proj")
                .join("modules")
                .join("core")
                .join("module.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(mj["name"].as_str().unwrap(), "core");
    assert!(
        mj["main"].as_str().is_some(),
        "module.json must have a 'main' field"
    );
}

#[test]
fn init_gitignore_excludes_lib_and_dist() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "gitignore-proj"]);

    let gi =
        fs::read_to_string(tmp.path().join("gitignore-proj").join(".gitignore")).unwrap();
    assert!(gi.contains("lib/"), ".gitignore must exclude lib/");
    assert!(
        gi.contains("dist") || gi.contains("src/dist"),
        ".gitignore must exclude dist"
    );
}

// ── nexa build ───────────────────────────────────────────────────────────────

/// The dist dir is `<project_root>/dist/<module_name>/`.
fn dist_dir(proj_dir: &Path, module: &str) -> PathBuf {
    proj_dir.join("dist").join(module)
}

#[test]
fn build_compiles_default_module() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "build-proj"]);
    let proj_dir = tmp.path().join("build-proj");

    nexa_ok(&proj_dir, &["build"]);

    let dist = dist_dir(&proj_dir, "core");
    assert!(dist.join("app.js").exists(), "dist/core/app.js not produced after build");
}

#[test]
fn build_produces_index_html() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "html-proj"]);
    let proj_dir = tmp.path().join("html-proj");

    nexa_ok(&proj_dir, &["build"]);

    let dist = dist_dir(&proj_dir, "core");
    assert!(
        dist.join("index.html").exists(),
        "dist/core/index.html not produced after build"
    );
}

#[test]
fn build_incremental_second_run_skips_unchanged() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "incremental-proj"]);
    let proj_dir = tmp.path().join("incremental-proj");

    // First build — must compile.
    let out1 = nexa_ok(&proj_dir, &["build"]);
    let s1 = String::from_utf8_lossy(&out1.stdout);
    assert!(
        s1.contains("compiled") || s1.contains("Build OK"),
        "first build should compile: {s1}"
    );

    // Second build — sources unchanged, should be a no-op.
    let out2 = nexa_ok(&proj_dir, &["build"]);
    let s2 = String::from_utf8_lossy(&out2.stdout);
    assert!(
        s2.contains("up to date") || s2.contains("nothing to compile"),
        "second build should skip unchanged modules: {s2}"
    );
}

#[test]
fn build_recompiles_after_source_change() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "recompile-proj"]);
    let proj_dir = tmp.path().join("recompile-proj");

    // First build.
    nexa_ok(&proj_dir, &["build"]);

    // Modify the entry file to trigger recompilation.
    let entry = proj_dir
        .join("modules")
        .join("core")
        .join("src")
        .join("main")
        .join("app.nx");
    let original = fs::read_to_string(&entry).unwrap();
    fs::write(&entry, format!("{original}\n// e2e recompile marker\n")).unwrap();

    let out = nexa_ok(&proj_dir, &["build"]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("compiled") || s.contains("Build OK"),
        "modified source should trigger recompile: {s}"
    );
}

// ── nexa module add ───────────────────────────────────────────────────────────

#[test]
fn module_add_creates_module_layout() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "modular-proj"]);
    let proj_dir = tmp.path().join("modular-proj");

    nexa_ok(&proj_dir, &["module", "add", "api"]);

    assert!(
        proj_dir
            .join("modules")
            .join("api")
            .join("module.json")
            .exists(),
        "modules/api/module.json not created"
    );
    assert!(
        proj_dir
            .join("modules")
            .join("api")
            .join("src")
            .join("main")
            .exists(),
        "modules/api/src/main/ not created"
    );

    let pj: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(proj_dir.join("project.json")).unwrap(),
    )
    .unwrap();
    let mods = pj["modules"].as_array().unwrap();
    assert!(
        mods.contains(&serde_json::json!("api")),
        "project.json modules array must include 'api' after module add"
    );
}

// ── nexa package ─────────────────────────────────────────────────────────────

#[test]
fn package_creates_nexa_bundle() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "pkg-proj"]);
    let proj_dir = tmp.path().join("pkg-proj");

    let out_path = proj_dir.join("pkg-proj-core.nexa");
    nexa_ok(
        &proj_dir,
        &["package", "--output", out_path.to_str().unwrap()],
    );

    assert!(out_path.exists(), ".nexa bundle not created");
    // Verify it's a valid ZIP by opening it.
    let file = std::fs::File::open(&out_path).unwrap();
    let archive = zip::ZipArchive::new(file);
    assert!(archive.is_ok(), ".nexa file is not a valid ZIP");
    let mut archive = archive.unwrap();
    assert!(archive.by_name("app.nxb").is_ok(), "app.nxb missing from bundle");
    assert!(
        archive.by_name("manifest.json").is_ok(),
        "manifest.json missing from bundle"
    );
    assert!(
        archive.by_name("signature.sig").is_ok(),
        "signature.sig missing from bundle"
    );
}

#[test]
fn package_manifest_contains_project_name() {
    let tmp = TempDir::new().unwrap();
    nexa_ok(tmp.path(), &["init", "named-proj"]);
    let proj_dir = tmp.path().join("named-proj");

    let out_path = proj_dir.join("named-proj-core.nexa");
    nexa_ok(
        &proj_dir,
        &["package", "--output", out_path.to_str().unwrap()],
    );

    let file = std::fs::File::open(&out_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut manifest_entry = archive.by_name("manifest.json").unwrap();
    let mut manifest_str = String::new();
    std::io::Read::read_to_string(&mut manifest_entry, &mut manifest_str).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str).unwrap();

    assert_eq!(
        manifest["name"].as_str().unwrap(),
        "named-proj-core",
        "manifest name should be '<project>-<module>'"
    );
    assert_eq!(
        manifest["version"].as_str().unwrap(),
        "0.1.0",
        "manifest version should match project.json"
    );
}
