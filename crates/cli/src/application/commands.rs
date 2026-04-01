use crate::application::project::NexaProject;
use nexa_compiler::compile_project_file;
use nexa_server::{AppState, build_router};
use notify::{Config as WatchConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn load_project(dir: Option<PathBuf>) -> NexaProject {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    NexaProject::load(&dir).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    })
}

pub fn build(project_dir: Option<PathBuf>) {
    let proj = load_project(project_dir);
    println!("Compiling {} ...", proj.entry_file().display());
    match compile_project_file(&proj.entry_file(), &proj.src_root()) {
        Ok(result) => write_dist(&proj.dist_dir(), result),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

pub async fn run(project_dir: Option<PathBuf>, port_override: Option<u16>, watch: bool) {
    let proj = load_project(project_dir);
    println!("Compiling {} ...", proj.entry_file().display());
    let result = match compile_project_file(&proj.entry_file(), &proj.src_root()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let dist = proj.dist_dir();
    let _ = fs::create_dir_all(&dist);
    let _ = fs::write(dist.join("index.html"), &result.html);
    let _ = fs::write(dist.join("app.js"),     &result.js);

    let port  = port_override.unwrap_or(3000);
    let state = Arc::new(AppState::new(result.html, result.js, port));

    if watch {
        println!("Watch mode — watching {}", proj.src_root().display());
        let state_clone = state.clone();
        let proj_clone  = proj.clone();
        tokio::spawn(async move {
            watch_task(state_clone, proj_clone).await;
        });
    }

    let router   = build_router(state);
    let addr     = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| {
        eprintln!("Cannot bind to {addr}: {e}");
        std::process::exit(1);
    });
    println!("Nexa dev server → http://localhost:{port}");
    axum::serve(listener, router.into_make_service()).await.unwrap();
}

/// Watches `src/` for `.nx` changes, recompiles, and broadcasts "reload"
/// to all connected WebSocket clients.
async fn watch_task(state: Arc<AppState>, proj: NexaProject) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(32);

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, _>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        },
        WatchConfig::default(),
    )
    .unwrap_or_else(|e| {
        eprintln!("Watch error: {e}");
        std::process::exit(1);
    });

    watcher
        .watch(&proj.src_root(), RecursiveMode::Recursive)
        .unwrap_or_else(|e| {
            eprintln!("Watch error: {e}");
            std::process::exit(1);
        });

    while let Some(event) = rx.recv().await {
        let has_nx = event
            .paths
            .iter()
            .any(|p| p.extension().map(|e| e == "nx").unwrap_or(false));

        if !has_nx {
            continue;
        }

        println!("Change detected, recompiling...");
        match compile_project_file(&proj.entry_file(), &proj.src_root()) {
            Ok(result) => {
                state.update(result.html, result.js).await;
                println!("Recompile OK — reload sent");
            }
            Err(e) => eprintln!("{e}"),
        }
    }
}

pub fn write_dist(dist_dir: &Path, result: nexa_compiler::CompileResult) {
    fs::create_dir_all(dist_dir).expect("cannot create dist/");
    fs::write(dist_dir.join("index.html"), &result.html).expect("cannot write index.html");
    fs::write(dist_dir.join("app.js"),     &result.js).expect("cannot write app.js");
    println!("Build OK → {}", dist_dir.display());
}
