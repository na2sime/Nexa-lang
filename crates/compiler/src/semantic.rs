//! Semantic analyser.
//!
//! Checks:
//!   - No duplicate class/interface names
//!   - extends/implements refer to existing names
//!   - Routes point to Window declarations
//!   - Imported symbols exist (names only — full type-checking is future work)

use crate::ast::*;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("Undefined type: '{0}'")]
    UndefinedType(String),
    #[error("Duplicate declaration: '{0}'")]
    Duplicate(String),
    #[error("Route target '{0}' is not a window")]
    NotAWindow(String),
    #[error("Import '{0}' refers to unknown symbol")]
    UnknownImport(String),
    #[error("Symbol '{0}' is not public and cannot be imported")]
    NotPublic(String),
}

pub struct SemanticAnalyzer {
    classes: HashMap<String, ClassDecl>,
    interfaces: HashMap<String, InterfaceDecl>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer { classes: HashMap::new(), interfaces: HashMap::new() }
    }

    pub fn analyze(&mut self, program: &Program) -> Result<(), SemanticError> {
        // ── Pass 1: collect all names ───────────────────────────────────────
        for decl in &program.declarations {
            match decl {
                Declaration::Class(cls) => {
                    if self.classes.contains_key(&cls.name) {
                        return Err(SemanticError::Duplicate(cls.name.clone()));
                    }
                    self.classes.insert(cls.name.clone(), cls.clone());
                }
                Declaration::Interface(iface) => {
                    if self.interfaces.contains_key(&iface.name) {
                        return Err(SemanticError::Duplicate(iface.name.clone()));
                    }
                    self.interfaces.insert(iface.name.clone(), iface.clone());
                }
            }
        }

        // ── Pass 2: validate references ─────────────────────────────────────
        for decl in &program.declarations {
            if let Declaration::Class(cls) = decl {
                self.check_class(cls)?;
            }
        }

        // ── Pass 3: validate imports ────────────────────────────────────────
        // The resolver has already merged imported declarations into `program.declarations`,
        // so we just check that each import path's last segment resolves to a known symbol.
        let all_names: HashSet<&str> = self.classes.keys()
            .map(|s| s.as_str())
            .chain(self.interfaces.keys().map(|s| s.as_str()))
            .collect();

        for import in &program.imports {
            let symbol = import.path.split('.').last().unwrap_or("");
            if !all_names.contains(symbol) {
                return Err(SemanticError::UnknownImport(import.path.clone()));
            }
            // Check it's public
            if let Some(cls) = self.classes.get(symbol) {
                if cls.visibility != Visibility::Public {
                    return Err(SemanticError::NotPublic(symbol.to_string()));
                }
            }
        }

        // ── Pass 4: validate routes ─────────────────────────────────────────
        for route in &program.routes {
            match self.classes.get(&route.target) {
                None => return Err(SemanticError::UndefinedType(route.target.clone())),
                Some(cls) if cls.kind != ClassKind::Window => {
                    return Err(SemanticError::NotAWindow(route.target.clone()));
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn check_class(&self, cls: &ClassDecl) -> Result<(), SemanticError> {
        if let Some(parent) = &cls.extends {
            if !self.classes.contains_key(parent) {
                return Err(SemanticError::UndefinedType(parent.clone()));
            }
        }
        for iface in &cls.implements {
            if !self.interfaces.contains_key(iface) {
                return Err(SemanticError::UndefinedType(iface.clone()));
            }
        }
        Ok(())
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self { Self::new() }
}
