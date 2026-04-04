//! Semantic analyser.
//!
//! Checks:
//!   - No duplicate class/interface names
//!   - extends/implements refer to existing names
//!   - Routes point to Window declarations
//!   - Imported symbols exist (names only — full type-checking is future work)
//!   - Type mismatches in `let` annotations and `return` statements (Pass 5)

use crate::domain::ast::*;
use crate::domain::span::Span;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("Undefined type '{name}'")]
    UndefinedType { name: String, span: Span },
    #[error("Duplicate declaration '{name}'")]
    Duplicate { name: String, span: Span },
    #[error("Route target '{name}' is not a window")]
    NotAWindow { name: String, span: Span },
    #[error("Import '{path}' refers to unknown symbol")]
    UnknownImport { path: String, span: Span },
    #[error("Symbol '{name}' is not public and cannot be imported")]
    NotPublic { name: String, span: Span },
    #[error("Type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String, span: Span },
}

impl SemanticError {
    pub fn span(&self) -> Span {
        match self {
            SemanticError::UndefinedType { span, .. } => *span,
            SemanticError::Duplicate { span, .. } => *span,
            SemanticError::NotAWindow { span, .. } => *span,
            SemanticError::UnknownImport { span, .. } => *span,
            SemanticError::NotPublic { span, .. } => *span,
            SemanticError::TypeMismatch { span, .. } => *span,
        }
    }
}

pub struct SemanticAnalyzer {
    classes: HashMap<String, ClassDecl>,
    interfaces: HashMap<String, InterfaceDecl>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            classes: HashMap::new(),
            interfaces: HashMap::new(),
        }
    }

    pub fn analyze(&mut self, program: &Program) -> Result<(), SemanticError> {
        // ── Pass 1: collect all names ───────────────────────────────────────
        for decl in &program.declarations {
            match decl {
                Declaration::Class(cls) => {
                    if self.classes.contains_key(&cls.name) {
                        return Err(SemanticError::Duplicate {
                            name: cls.name.clone(),
                            span: Span::dummy(),
                        });
                    }
                    self.classes.insert(cls.name.clone(), cls.clone());
                }
                Declaration::Interface(iface) => {
                    if self.interfaces.contains_key(&iface.name) {
                        return Err(SemanticError::Duplicate {
                            name: iface.name.clone(),
                            span: Span::dummy(),
                        });
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
        let all_names: HashSet<&str> = self
            .classes
            .keys()
            .map(|s| s.as_str())
            .chain(self.interfaces.keys().map(|s| s.as_str()))
            .collect();

        for import in &program.imports {
            let symbol = import.path.split('.').next_back().unwrap_or("");
            if !all_names.contains(symbol) {
                return Err(SemanticError::UnknownImport {
                    path: import.path.clone(),
                    span: Span::dummy(),
                });
            }
            // Check it's public
            if let Some(cls) = self.classes.get(symbol) {
                if cls.visibility != Visibility::Public {
                    return Err(SemanticError::NotPublic {
                        name: symbol.to_string(),
                        span: Span::dummy(),
                    });
                }
            }
        }

        // ── Pass 4: validate routes ─────────────────────────────────────────
        for route in &program.routes {
            match self.classes.get(&route.target) {
                None => {
                    return Err(SemanticError::UndefinedType {
                        name: route.target.clone(),
                        span: Span::dummy(),
                    })
                }
                Some(cls) if cls.kind != ClassKind::Window => {
                    return Err(SemanticError::NotAWindow {
                        name: route.target.clone(),
                        span: Span::dummy(),
                    });
                }
                _ => {}
            }
        }

        // ── Pass 5: type checking ───────────────────────────────────────────
        for decl in &program.declarations {
            if let Declaration::Class(cls) = decl {
                self.check_class_types(cls)?;
            }
        }

        Ok(())
    }

    fn check_class(&self, cls: &ClassDecl) -> Result<(), SemanticError> {
        if let Some(parent) = &cls.extends {
            if !self.classes.contains_key(parent) {
                return Err(SemanticError::UndefinedType {
                    name: parent.clone(),
                    span: Span::dummy(),
                });
            }
        }
        for iface in &cls.implements {
            if !self.interfaces.contains_key(iface) {
                return Err(SemanticError::UndefinedType {
                    name: iface.clone(),
                    span: Span::dummy(),
                });
            }
        }
        Ok(())
    }

    // ── Type checking (Pass 5) ────────────────────────────────────────────────

    fn check_class_types(&self, cls: &ClassDecl) -> Result<(), SemanticError> {
        // Seed the env with field types so method bodies can resolve them.
        let field_env: HashMap<String, Type> = cls
            .fields
            .iter()
            .map(|f| (f.name.clone(), f.ty.clone()))
            .collect();

        if let Some(ctor) = &cls.constructor {
            let mut env = field_env.clone();
            for p in &ctor.params {
                env.insert(p.name.clone(), p.ty.clone());
            }
            for stmt in &ctor.body {
                self.check_stmt_types(stmt, &Type::Void, cls, &mut env)?;
            }
        }

        for method in &cls.methods {
            let mut env = field_env.clone();
            for p in &method.params {
                env.insert(p.name.clone(), p.ty.clone());
            }
            for stmt in &method.body {
                self.check_stmt_types(stmt, &method.return_type, cls, &mut env)?;
            }
        }
        Ok(())
    }

    /// Infer the AST type of an expression given the current local environment.
    /// Returns `None` when the type cannot be determined statically (method calls, etc.).
    fn infer_expr_type(
        &self,
        expr: &Expr,
        cls: &ClassDecl,
        env: &HashMap<String, Type>,
    ) -> Option<Type> {
        match expr {
            Expr::IntLit(_) => Some(Type::Int),
            Expr::StringLit(_) => Some(Type::String),
            Expr::BoolLit(_) => Some(Type::Bool),
            Expr::Ident(name) => env.get(name).cloned(),
            Expr::This => Some(Type::Custom(cls.name.clone())),
            Expr::Binary { op, left, right } => {
                let lt = self.infer_expr_type(left, cls, env)?;
                let rt = self.infer_expr_type(right, cls, env)?;
                match op {
                    BinOp::Add => match (&lt, &rt) {
                        (Type::Int, Type::Int) => Some(Type::Int),
                        (Type::String, _) | (_, Type::String) => Some(Type::String),
                        _ => None,
                    },
                    BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        if lt == Type::Int && rt == Type::Int {
                            Some(Type::Int)
                        } else {
                            None
                        }
                    }
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::Le
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or => Some(Type::Bool),
                }
            }
            Expr::Unary { op, expr: inner } => match op {
                UnOp::Not => Some(Type::Bool),
                UnOp::Neg => {
                    let t = self.infer_expr_type(inner, cls, env)?;
                    if t == Type::Int {
                        Some(Type::Int)
                    } else {
                        None
                    }
                }
            },
            // Method calls, lambdas, blocks: type not inferable without a full type
            // system — skip checking, the lower pass will mark them Unknown.
            _ => None,
        }
    }

    fn check_stmt_types(
        &self,
        stmt: &Stmt,
        return_type: &Type,
        cls: &ClassDecl,
        env: &mut HashMap<String, Type>,
    ) -> Result<(), SemanticError> {
        match stmt {
            Stmt::Let { name, ty: Some(declared), init } => {
                if let Some(inferred) = self.infer_expr_type(init, cls, env) {
                    if &inferred != declared {
                        return Err(SemanticError::TypeMismatch {
                            expected: format!("{declared:?}"),
                            found: format!("{inferred:?}"),
                            span: Span::dummy(),
                        });
                    }
                }
                env.insert(name.clone(), declared.clone());
            }
            Stmt::Let { name, ty: None, init } => {
                // No annotation: infer and record for subsequent statements.
                if let Some(t) = self.infer_expr_type(init, cls, env) {
                    env.insert(name.clone(), t);
                }
            }
            Stmt::Return(Some(expr)) if return_type != &Type::Void => {
                if let Some(found) = self.infer_expr_type(expr, cls, env) {
                    if &found != return_type {
                        return Err(SemanticError::TypeMismatch {
                            expected: format!("{return_type:?}"),
                            found: format!("{found:?}"),
                            span: Span::dummy(),
                        });
                    }
                }
            }
            Stmt::If { then_body, else_body, .. } => {
                // Each branch gets its own child scope (inherits parent).
                let mut then_env = env.clone();
                for s in then_body {
                    self.check_stmt_types(s, return_type, cls, &mut then_env)?;
                }
                if let Some(eb) = else_body {
                    let mut else_env = env.clone();
                    for s in eb {
                        self.check_stmt_types(s, return_type, cls, &mut else_env)?;
                    }
                }
            }
            Stmt::While { body, .. } => {
                let mut inner_env = env.clone();
                for s in body {
                    self.check_stmt_types(s, return_type, cls, &mut inner_env)?;
                }
            }
            Stmt::For { var, body, .. } => {
                let mut inner_env = env.clone();
                // Element type unknown without knowing the iterator's item type.
                inner_env.insert(var.clone(), Type::Generic("T".into()));
                for s in body {
                    self.check_stmt_types(s, return_type, cls, &mut inner_env)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_class(name: &str, kind: ClassKind, vis: Visibility) -> ClassDecl {
        ClassDecl {
            visibility: vis,
            kind,
            name: name.into(),
            type_params: vec![],
            extends: None,
            implements: vec![],
            fields: vec![],
            constructor: None,
            methods: vec![],
        }
    }

    fn make_program(decls: Vec<Declaration>, routes: Vec<Route>) -> Program {
        Program {
            name: "test".into(),
            package: None,
            imports: vec![],
            server: None,
            declarations: decls,
            routes,
        }
    }

    // ── Pass 1-4 ─────────────────────────────────────────────────────────────

    #[test]
    fn duplicate_class_error() {
        let prog = make_program(
            vec![
                Declaration::Class(make_class("Foo", ClassKind::Class, Visibility::Public)),
                Declaration::Class(make_class("Foo", ClassKind::Class, Visibility::Public)),
            ],
            vec![],
        );
        let err = SemanticAnalyzer::new().analyze(&prog).unwrap_err();
        assert!(matches!(err, SemanticError::Duplicate { .. }));
    }

    #[test]
    fn route_must_target_window() {
        let prog = make_program(
            vec![Declaration::Class(make_class("Home", ClassKind::Class, Visibility::Public))],
            vec![Route { path: "/".into(), target: "Home".into() }],
        );
        let err = SemanticAnalyzer::new().analyze(&prog).unwrap_err();
        assert!(matches!(err, SemanticError::NotAWindow { .. }));
    }

    #[test]
    fn valid_window_route_passes() {
        let prog = make_program(
            vec![Declaration::Class(make_class("Home", ClassKind::Window, Visibility::Public))],
            vec![Route { path: "/".into(), target: "Home".into() }],
        );
        assert!(SemanticAnalyzer::new().analyze(&prog).is_ok());
    }

    // ── Pass 5: type checking ─────────────────────────────────────────────────

    #[test]
    fn let_type_mismatch_detected() {
        // let x: String = 42  → should fail
        let method = Method {
            visibility: Visibility::Public,
            name: "run".into(),
            params: vec![],
            return_type: Type::Void,
            body: vec![Stmt::Let {
                name: "x".into(),
                ty: Some(Type::String),
                init: Expr::IntLit(42),
            }],
        };
        let cls = ClassDecl {
            methods: vec![method],
            ..make_class("App", ClassKind::Class, Visibility::Public)
        };
        let prog = make_program(vec![Declaration::Class(cls)], vec![]);
        let err = SemanticAnalyzer::new().analyze(&prog).unwrap_err();
        assert!(
            matches!(err, SemanticError::TypeMismatch { .. }),
            "expected TypeMismatch, got {err:?}"
        );
    }

    #[test]
    fn let_no_annotation_infers_int() {
        // let x = 42  → should pass (inferred as Int)
        let method = Method {
            visibility: Visibility::Public,
            name: "run".into(),
            params: vec![],
            return_type: Type::Void,
            body: vec![Stmt::Let {
                name: "x".into(),
                ty: None,
                init: Expr::IntLit(42),
            }],
        };
        let cls = ClassDecl {
            methods: vec![method],
            ..make_class("App", ClassKind::Class, Visibility::Public)
        };
        let prog = make_program(vec![Declaration::Class(cls)], vec![]);
        assert!(SemanticAnalyzer::new().analyze(&prog).is_ok());
    }

    #[test]
    fn return_type_mismatch_detected() {
        // fn get(): Int { return "oops" }  → should fail
        let method = Method {
            visibility: Visibility::Public,
            name: "get".into(),
            params: vec![],
            return_type: Type::Int,
            body: vec![Stmt::Return(Some(Expr::StringLit("oops".into())))],
        };
        let cls = ClassDecl {
            methods: vec![method],
            ..make_class("App", ClassKind::Class, Visibility::Public)
        };
        let prog = make_program(vec![Declaration::Class(cls)], vec![]);
        let err = SemanticAnalyzer::new().analyze(&prog).unwrap_err();
        assert!(
            matches!(err, SemanticError::TypeMismatch { .. }),
            "expected TypeMismatch, got {err:?}"
        );
    }

    #[test]
    fn return_type_match_passes() {
        // fn get(): Int { return 7 }  → should pass
        let method = Method {
            visibility: Visibility::Public,
            name: "get".into(),
            params: vec![],
            return_type: Type::Int,
            body: vec![Stmt::Return(Some(Expr::IntLit(7)))],
        };
        let cls = ClassDecl {
            methods: vec![method],
            ..make_class("App", ClassKind::Class, Visibility::Public)
        };
        let prog = make_program(vec![Declaration::Class(cls)], vec![]);
        assert!(SemanticAnalyzer::new().analyze(&prog).is_ok());
    }

    #[test]
    fn inferred_let_used_in_binary() {
        // let x = 1; let y = x + 2  → should pass (y inferred as Int)
        let method = Method {
            visibility: Visibility::Public,
            name: "run".into(),
            params: vec![],
            return_type: Type::Void,
            body: vec![
                Stmt::Let { name: "x".into(), ty: None, init: Expr::IntLit(1) },
                Stmt::Let {
                    name: "y".into(),
                    ty: None,
                    init: Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(Expr::Ident("x".into())),
                        right: Box::new(Expr::IntLit(2)),
                    },
                },
            ],
        };
        let cls = ClassDecl {
            methods: vec![method],
            ..make_class("App", ClassKind::Class, Visibility::Public)
        };
        let prog = make_program(vec![Declaration::Class(cls)], vec![]);
        assert!(SemanticAnalyzer::new().analyze(&prog).is_ok());
    }
}
