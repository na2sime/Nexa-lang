use serde::Deserialize;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use thiserror::Error;

// ── Erreurs ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project.json introuvable dans '{0}' — êtes-vous dans un projet Nexa ?")]
    MissingProjectJson(PathBuf),

    #[error("nexa-compiler.yaml introuvable dans '{0}'")]
    MissingCompilerYaml(PathBuf),

    #[error("répertoire src/main/ introuvable dans '{0}'")]
    MissingSrcMain(PathBuf),

    #[error("fichier d'entrée introuvable : '{0}'")]
    MissingEntryFile(PathBuf),

    #[error("lecture project.json : {0}")]
    ReadProjectJson(#[source] std::io::Error),

    #[error("parse project.json : {0}")]
    ParseProjectJson(#[source] serde_json::Error),

    #[error("lecture nexa-compiler.yaml : {0}")]
    ReadCompilerYaml(#[source] std::io::Error),

    #[error("parse nexa-compiler.yaml : {0}")]
    ParseCompilerYaml(#[source] serde_yaml::Error),
}

// ── Structs de config ─────────────────────────────────────────────────────────

/// Désérialisé depuis `project.json`
#[derive(Debug, Deserialize, PartialEq)]
#[allow(dead_code)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub author: String,
    /// Nom du fichier d'entrée dans `src/main/`, ex: "app.nexa"
    pub main: String,
    /// Réservé pour les futures dépendances
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Désérialisé depuis `nexa-compiler.yaml`
#[derive(Debug, Deserialize, PartialEq)]
#[allow(dead_code)]
pub struct CompilerConfig {
    pub version: String,
}

// ── Projet ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct NexaProject {
    root: PathBuf,
    pub project: ProjectConfig,
    pub compiler: CompilerConfig,
}

// ── Fonctions de parsing pures (testables indépendamment) ─────────────────────

fn parse_project_config(text: &str) -> Result<ProjectConfig, ProjectError> {
    serde_json::from_str(text).map_err(ProjectError::ParseProjectJson)
}

fn parse_compiler_config(text: &str) -> Result<CompilerConfig, ProjectError> {
    serde_yaml::from_str(text).map_err(ProjectError::ParseCompilerYaml)
}

// ── Implémentation ────────────────────────────────────────────────────────────

impl NexaProject {
    /// Charge et valide un projet depuis `dir`.
    ///
    /// Validation (fatale, dans l'ordre) :
    ///   1. `project.json` — présent + parseable
    ///   2. `nexa-compiler.yaml` — présent + parseable
    ///   3. `src/main/` — répertoire présent
    ///   4. Fichier `main` déclaré dans `project.json` — présent
    ///
    /// Side-effects (non bloquants) : auto-création de `src/.nexa/`, `src/libs/`, `src/test/`
    pub fn load(dir: &Path) -> Result<Self, ProjectError> {
        let root = dir.to_path_buf();

        let project = fs::read_to_string(root.join("project.json"))
            .map_err(|e| match e.kind() {
                ErrorKind::NotFound => ProjectError::MissingProjectJson(root.clone()),
                _ => ProjectError::ReadProjectJson(e),
            })
            .and_then(|t| parse_project_config(&t))?;

        let compiler = fs::read_to_string(root.join("nexa-compiler.yaml"))
            .map_err(|e| match e.kind() {
                ErrorKind::NotFound => ProjectError::MissingCompilerYaml(root.clone()),
                _ => ProjectError::ReadCompilerYaml(e),
            })
            .and_then(|t| parse_compiler_config(&t))?;

        let src_main = root.join("src").join("main");
        if !src_main.is_dir() {
            return Err(ProjectError::MissingSrcMain(root));
        }

        let entry = src_main.join(&project.main);
        if !entry.exists() {
            return Err(ProjectError::MissingEntryFile(entry));
        }

        let proj = NexaProject { root, project, compiler };
        proj.ensure_optional_dirs();
        Ok(proj)
    }

    /// Crée silencieusement les répertoires optionnels s'ils n'existent pas encore.
    fn ensure_optional_dirs(&self) {
        for d in &[
            self.src_root().join(".nexa"),
            self.src_root().join("libs"),
            self.src_root().join("test"),
        ] {
            let _ = fs::create_dir_all(d);
        }
    }

    /// `<root>/src/`
    pub fn src_root(&self) -> PathBuf {
        self.root.join("src")
    }

    /// `<root>/src/main/<project.main>`
    pub fn entry_file(&self) -> PathBuf {
        self.src_root().join("main").join(&self.project.main)
    }

    /// `<root>/src/dist/`
    pub fn dist_dir(&self) -> PathBuf {
        self.src_root().join("dist")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── Helper ────────────────────────────────────────────────────────────────

    /// Crée un projet Nexa valide dans `dir`.
    fn make_valid_project(dir: &Path) {
        fs::write(
            dir.join("project.json"),
            r#"{"name":"test-app","version":"0.1.0","author":"Tester","main":"app.nexa"}"#,
        )
        .unwrap();
        fs::write(dir.join("nexa-compiler.yaml"), "version: \"0.1\"\n").unwrap();
        let src_main = dir.join("src").join("main");
        fs::create_dir_all(&src_main).unwrap();
        fs::write(src_main.join("app.nexa"), "").unwrap();
    }

    // ── parse_project_config ──────────────────────────────────────────────────

    #[test]
    fn parse_project_config_valid() {
        let json = r#"{"name":"my-app","version":"1.0.0","author":"Dev","main":"app.nexa"}"#;
        let cfg = parse_project_config(json).unwrap();
        assert_eq!(cfg.name, "my-app");
        assert_eq!(cfg.version, "1.0.0");
        assert_eq!(cfg.author, "Dev");
        assert_eq!(cfg.main, "app.nexa");
        assert!(cfg.dependencies.is_empty());
    }

    #[test]
    fn parse_project_config_dependencies_optional() {
        let json = r#"{"name":"a","version":"1","author":"b","main":"m.nexa"}"#;
        let cfg = parse_project_config(json).unwrap();
        assert!(cfg.dependencies.is_empty());
    }

    #[test]
    fn parse_project_config_with_dependencies() {
        let json = r#"{"name":"a","version":"1","author":"b","main":"m.nexa","dependencies":["lib1","lib2"]}"#;
        let cfg = parse_project_config(json).unwrap();
        assert_eq!(cfg.dependencies, vec!["lib1", "lib2"]);
    }

    #[test]
    fn parse_project_config_missing_required_field() {
        // "main" manquant
        let json = r#"{"name":"a","version":"1","author":"b"}"#;
        assert!(parse_project_config(json).is_err());
    }

    #[test]
    fn parse_project_config_invalid_json() {
        assert!(parse_project_config("pas du json").is_err());
    }

    // ── parse_compiler_config ─────────────────────────────────────────────────

    #[test]
    fn parse_compiler_config_valid() {
        let cfg = parse_compiler_config("version: \"0.1\"\n").unwrap();
        assert_eq!(cfg.version, "0.1");
    }

    #[test]
    fn parse_compiler_config_missing_version() {
        assert!(parse_compiler_config("autre_champ: true").is_err());
    }

    #[test]
    fn parse_compiler_config_invalid_yaml() {
        assert!(parse_compiler_config(":\n  bad:\n  yaml:").is_err());
    }

    // ── Helpers de chemin (purs, sans I/O) ───────────────────────────────────

    #[test]
    fn path_helpers_are_correct() {
        let tmp = TempDir::new().unwrap();
        make_valid_project(tmp.path());
        let proj = NexaProject::load(tmp.path()).unwrap();
        assert_eq!(proj.src_root(),   tmp.path().join("src"));
        assert_eq!(proj.entry_file(), tmp.path().join("src").join("main").join("app.nexa"));
        assert_eq!(proj.dist_dir(),   tmp.path().join("src").join("dist"));
    }

    // ── NexaProject::load — succès ────────────────────────────────────────────

    #[test]
    fn load_valid_project_succeeds() {
        let tmp = TempDir::new().unwrap();
        make_valid_project(tmp.path());
        assert!(NexaProject::load(tmp.path()).is_ok());
    }

    #[test]
    fn load_creates_optional_dirs() {
        let tmp = TempDir::new().unwrap();
        make_valid_project(tmp.path());
        NexaProject::load(tmp.path()).unwrap();
        assert!(tmp.path().join("src").join(".nexa").is_dir(), "src/.nexa/ doit être créé");
        assert!(tmp.path().join("src").join("libs").is_dir(),  "src/libs/ doit être créé");
        assert!(tmp.path().join("src").join("test").is_dir(),  "src/test/ doit être créé");
    }

    // ── NexaProject::load — cas d'erreur ─────────────────────────────────────

    #[test]
    fn load_missing_project_json() {
        let tmp = TempDir::new().unwrap();
        let err = NexaProject::load(tmp.path()).unwrap_err();
        assert!(matches!(err, ProjectError::MissingProjectJson(_)));
    }

    #[test]
    fn load_invalid_project_json() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("project.json"), "pas du json").unwrap();
        fs::write(tmp.path().join("nexa-compiler.yaml"), "version: \"0.1\"").unwrap();
        let err = NexaProject::load(tmp.path()).unwrap_err();
        assert!(matches!(err, ProjectError::ParseProjectJson(_)));
    }

    #[test]
    fn load_missing_compiler_yaml() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("project.json"),
            r#"{"name":"a","version":"1","author":"b","main":"m.nexa"}"#,
        )
        .unwrap();
        let err = NexaProject::load(tmp.path()).unwrap_err();
        assert!(matches!(err, ProjectError::MissingCompilerYaml(_)));
    }

    #[test]
    fn load_missing_src_main() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("project.json"),
            r#"{"name":"a","version":"1","author":"b","main":"m.nexa"}"#,
        )
        .unwrap();
        fs::write(tmp.path().join("nexa-compiler.yaml"), "version: \"0.1\"").unwrap();
        let err = NexaProject::load(tmp.path()).unwrap_err();
        assert!(matches!(err, ProjectError::MissingSrcMain(_)));
    }

    #[test]
    fn load_missing_entry_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("project.json"),
            r#"{"name":"a","version":"1","author":"b","main":"absent.nexa"}"#,
        )
        .unwrap();
        fs::write(tmp.path().join("nexa-compiler.yaml"), "version: \"0.1\"").unwrap();
        fs::create_dir_all(tmp.path().join("src").join("main")).unwrap();
        // absent.nexa n'est pas créé
        let err = NexaProject::load(tmp.path()).unwrap_err();
        assert!(matches!(err, ProjectError::MissingEntryFile(_)));
    }
}
