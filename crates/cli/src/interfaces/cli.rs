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
    /// Compile le projet et démarre le dev server (accepte aussi un bundle .nexa)
    Run {
        /// Fichier .nexa à exécuter directement (optionnel)
        #[arg(value_name = "BUNDLE")]
        bundle: Option<PathBuf>,
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
    /// Empaquète le projet dans un bundle distribuable .nexa
    Package {
        /// Répertoire racine du projet (défaut : répertoire courant)
        #[arg(short, long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Chemin de sortie du fichier .nexa (défaut : <name>.nexa)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Crée un compte sur le registry
    Register {
        /// URL du registry (défaut : https://registry.nexa-lang.org)
        #[arg(long, value_name = "URL")]
        registry: Option<String>,
    },
    /// Connexion au registry
    Login {
        /// URL du registry (défaut : https://registry.nexa-lang.org)
        #[arg(long, value_name = "URL")]
        registry: Option<String>,
    },
    /// Publie le projet sur le registry
    Publish {
        /// Répertoire racine du projet (défaut : répertoire courant)
        #[arg(short, long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// URL du registry (défaut : credentials ou https://registry.nexa-lang.org)
        #[arg(long, value_name = "URL")]
        registry: Option<String>,
    },
    /// Installe des dépendances depuis le registry
    Install {
        /// Package à installer, ex: my-lib ou my-lib@1.0.0 (défaut : toutes les deps de project.json)
        #[arg(value_name = "PACKAGE")]
        package: Option<String>,
        /// Répertoire racine du projet (défaut : répertoire courant)
        #[arg(short, long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
}

pub async fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { bundle, project, port, watch } => {
            commands::run(bundle, project, port, watch).await
        }
        Commands::Build { project }             => commands::build(project),
        Commands::Package { project, output }   => commands::package(project, output),
        Commands::Register { registry }         => commands::register(registry),
        Commands::Login { registry }            => commands::login(registry),
        Commands::Publish { project, registry } => commands::publish(project, registry),
        Commands::Install { package, project }  => commands::install(package, project),
    }
}
