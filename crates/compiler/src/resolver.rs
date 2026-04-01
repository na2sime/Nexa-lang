//! Multi-file resolver.
//!
//! Maps package paths like `com.myapp.models.User` to file system paths,
//! parses them recursively, detects cycles, and merges all declarations into
//! a single flat `Program` that the semantic analyser and codegen can consume.

use crate::ast::{Declaration, ImportDecl, Program};
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("IO error loading '{0}': {1}")]
    Io(String, std::io::Error),
    #[error("Lex error in '{0}': {1}")]
    Lex(String, crate::lexer::LexError),
    #[error("Parse error in '{0}': {1}")]
    Parse(String, crate::parser::ParseError),
    #[error("Circular import detected: {0}")]
    Cycle(String),
    #[error("Cannot resolve import '{0}' (tried: {1})")]
    NotFound(String, String),
}

pub struct Resolver {
    /// Root directory for package resolution
    root: PathBuf,
    /// Cache: canonical file path → parsed declarations
    cache: HashMap<PathBuf, Vec<Declaration>>,
    /// Cycle detection: currently being loaded
    loading: HashSet<PathBuf>,
}

impl Resolver {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Resolver {
            root: root.into(),
            cache: HashMap::new(),
            loading: HashSet::new(),
        }
    }

    /// Resolve the entry program + all its (transitive) imports.
    /// Returns the entry `Program` with all imported declarations merged in.
    pub fn resolve(&mut self, entry: &Program, entry_path: &Path) -> Result<Program, ResolveError> {
        let entry_root = entry_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut merged_decls: Vec<Declaration> = Vec::new();

        // Load all imported declarations first (depth-first)
        for import in &entry.imports {
            self.load_import(import, &entry_root, &mut merged_decls)?;
        }

        // Append the entry file's own declarations last (so they win on name conflicts)
        merged_decls.extend(entry.declarations.clone());

        Ok(Program {
            name: entry.name.clone(),
            package: entry.package.clone(),
            imports: entry.imports.clone(),
            server: entry.server.clone(),
            declarations: merged_decls,
            routes: entry.routes.clone(),
        })
    }

    fn load_import(
        &mut self,
        import: &ImportDecl,
        relative_root: &Path,
        out: &mut Vec<Declaration>,
    ) -> Result<(), ResolveError> {
        let file_path = self.resolve_path(&import.path, relative_root)?;

        // Cycle detection
        if self.loading.contains(&file_path) {
            return Err(ResolveError::Cycle(import.path.clone()));
        }

        // Cache hit
        if let Some(decls) = self.cache.get(&file_path) {
            out.extend(decls.clone());
            return Ok(());
        }

        // Load and parse
        self.loading.insert(file_path.clone());

        let source = std::fs::read_to_string(&file_path)
            .map_err(|e| ResolveError::Io(file_path.display().to_string(), e))?;

        let tokens = Lexer::new(&source).tokenize()
            .map_err(|e| ResolveError::Lex(file_path.display().to_string(), e))?;

        let lib = Parser::new(tokens).parse_lib()
            .map_err(|e| ResolveError::Parse(file_path.display().to_string(), e))?;

        // Recursively resolve this file's imports
        let lib_root = file_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut lib_decls: Vec<Declaration> = Vec::new();
        for sub_import in &lib.imports {
            self.load_import(sub_import, &lib_root, &mut lib_decls)?;
        }
        lib_decls.extend(lib.declarations.clone());

        self.loading.remove(&file_path);
        self.cache.insert(file_path, lib_decls.clone());
        out.extend(lib_decls);
        Ok(())
    }

    /// Convert a dotted import path to a file system path.
    ///
    /// Strategy (in order):
    /// 1. Relative to the importing file's directory: `User` → `./User.nexa`
    /// 2. Package path from project root: `com.myapp.models.User` → `{root}/com/myapp/models/User.nexa`
    ///    or `{root}/com/myapp/models.nexa` (the segment is the last part of a file)
    fn resolve_path(&self, import_path: &str, relative_root: &Path) -> Result<PathBuf, ResolveError> {
        let parts: Vec<&str> = import_path.split('.').collect();

        // Try: relative directory / last_part.nexa
        let simple = relative_root.join(format!("{}.nexa", parts.last().unwrap_or(&"")));
        if simple.exists() {
            return Ok(simple.canonicalize().unwrap_or(simple));
        }

        // Try: relative root / all parts as dirs / last.nexa
        let mut rel_path = relative_root.to_path_buf();
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                rel_path.push(format!("{}.nexa", part));
            } else {
                rel_path.push(part);
            }
        }
        if rel_path.exists() {
            return Ok(rel_path.canonicalize().unwrap_or(rel_path));
        }

        // Try: project root / all parts as dirs / last.nexa
        let mut pkg_path = self.root.clone();
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                pkg_path.push(format!("{}.nexa", part));
            } else {
                pkg_path.push(part);
            }
        }
        if pkg_path.exists() {
            return Ok(pkg_path.canonicalize().unwrap_or(pkg_path));
        }

        Err(ResolveError::NotFound(
            import_path.to_string(),
            format!("tried: {}, {}, {}", simple.display(), rel_path.display(), pkg_path.display()),
        ))
    }
}
