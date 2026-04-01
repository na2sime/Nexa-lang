mod project;

use clap::{Parser, Subcommand};
use nexa_compiler::compile_project_file;
use nexa_server::{AppState, build_router};
use project::NexaProject;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "nexa", about = "Nexa language compiler & dev server", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile le projet et démarre le dev server
    Run {
        /// Répertoire racine du projet (défaut : répertoire courant)
        #[arg(short, long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Port du serveur (défaut : 3000)
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Compile le projet — écrit la sortie dans <project>/src/dist/
    Build {
        /// Répertoire racine du projet (défaut : répertoire courant)
        #[arg(short, long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { project, port } => run(project, port).await,
        Commands::Build { project }     => build(project),
    }
}

fn load_project(dir: Option<PathBuf>) -> NexaProject {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    NexaProject::load(&dir).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    })
}

fn build(project_dir: Option<PathBuf>) {
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

async fn run(project_dir: Option<PathBuf>, port_override: Option<u16>) {
    let proj = load_project(project_dir);
    println!("Compiling {} ...", proj.entry_file().display());
    let result = match compile_project_file(&proj.entry_file(), &proj.src_root()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let dist = proj.dist_dir();
    let _ = fs::create_dir_all(&dist);
    let _ = fs::write(dist.join("index.html"), &result.html);
    let _ = fs::write(dist.join("app.js"),     &result.js);

    let port = port_override.unwrap_or(3000);
    let state = AppState { html: result.html, js: result.js, port };
    let router = build_router(state);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| {
        eprintln!("Cannot bind to {addr}: {e}");
        std::process::exit(1);
    });
    println!("Nexa dev server → http://localhost:{port}");
    axum::serve(listener, router.into_make_service()).await.unwrap();
}

fn write_dist(dist_dir: &Path, result: nexa_compiler::CompileResult) {
    fs::create_dir_all(dist_dir).expect("cannot create dist/");
    fs::write(dist_dir.join("index.html"), &result.html).expect("cannot write index.html");
    fs::write(dist_dir.join("app.js"),     &result.js).expect("cannot write app.js");
    println!("Build OK → {}", dist_dir.display());
}
