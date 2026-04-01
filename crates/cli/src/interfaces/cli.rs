use crate::application::commands;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        /// Recompile et recharge le navigateur à chaque sauvegarde
        #[arg(long)]
        watch: bool,
    },
    /// Compile le projet — écrit la sortie dans <project>/src/dist/
    Build {
        /// Répertoire racine du projet (défaut : répertoire courant)
        #[arg(short, long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
}

pub async fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { project, port, watch } => commands::run(project, port, watch).await,
        Commands::Build { project }            => commands::build(project),
    }
}
