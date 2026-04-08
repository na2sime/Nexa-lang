//! IR → Rust source transpiler.
//!
//! Converts an [`IrModule`] from a `backend` or `cli` app into:
//! - `main.rs` : compilable Rust source
//! - `Cargo.toml` : workspace-independent manifest with required crate deps

use crate::domain::ir::{
    IrBinOp, IrClass, IrExpr, IrModule, IrStmt, IrType, IrUnOp,
};
use std::fmt::Write as FmtWrite;

// ── Public API ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum RustCodegenError {
    #[error("no app class found in module '{0}'")]
    NoAppClass(String),
    #[error("no main() method found in app class '{0}'")]
    NoMainMethod(String),
    #[error("internal fmt error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

#[derive(Debug)]
pub struct RustCodegenResult {
    /// Complete `main.rs` source.
    pub main_rs: String,
    /// Complete `Cargo.toml` contents.
    pub cargo_toml: String,
}

pub struct RustCodegen<'a> {
    /// Nexa module name (used in Cargo.toml [package]).
    module_name: &'a str,
    /// Project name (used in Cargo.toml [package]).
    project_name: &'a str,
    /// Project version (used in Cargo.toml [package]).
    project_version: &'a str,
}

impl<'a> RustCodegen<'a> {
    pub fn new(module_name: &'a str, project_name: &'a str, project_version: &'a str) -> Self {
        Self { module_name, project_name, project_version }
    }

    pub fn generate(&self, ir: &IrModule) -> Result<RustCodegenResult, RustCodegenError> {
        let app_class = ir
            .classes
            .iter()
            .find(|c| c.name == ir.name)
            .ok_or_else(|| RustCodegenError::NoAppClass(ir.name.clone()))?;

        let main_method = app_class
            .methods
            .iter()
            .find(|m| m.name == "main")
            .ok_or_else(|| RustCodegenError::NoMainMethod(app_class.name.clone()))?;

        let is_async = main_method.is_async;
        let mut out = String::new();

        // Emit helper structs for non-app classes
        for cls in ir.classes.iter().filter(|c| c.name != ir.name) {
            emit_class(&mut out, cls)?;
        }

        // Emit enums
        for en in &ir.enums {
            writeln!(out, "#[derive(Debug, Clone, PartialEq)]")?;
            writeln!(out, "pub enum {} {{", en.name)?;
            for variant in &en.variants {
                if variant.field_count == 0 {
                    writeln!(out, "    {},", variant.name)?;
                } else {
                    let fields = (0..variant.field_count).map(|_| "i64").collect::<Vec<_>>().join(", ");
                    writeln!(out, "    {}({}),", variant.name, fields)?;
                }
            }
            writeln!(out, "}}")?;
            writeln!(out)?;
        }

        // Emit main function
        if is_async {
            writeln!(out, "#[tokio::main]")?;
            writeln!(out, "async fn main() {{")?;
        } else {
            writeln!(out, "fn main() {{")?;
        }
        let mut body_out = String::new();
        emit_stmts(&mut body_out, &main_method.body, 1)?;
        out.push_str(&body_out);
        writeln!(out, "}}")?;

        let needs_tokio = is_async || uses_tokio(ir);
        let cargo_toml = self.generate_cargo_toml(needs_tokio);

        Ok(RustCodegenResult { main_rs: out, cargo_toml })
    }

    fn generate_cargo_toml(&self, needs_tokio: bool) -> String {
        let mut s = format!(
            r#"[package]
name = "{}-{}"
version = "{}"
edition = "2021"

[[bin]]
name = "{}"
path = "src/main.rs"

[dependencies]
"#,
            self.project_name,
            self.module_name,
            self.project_version,
            self.module_name,
        );
        if needs_tokio {
            s.push_str("tokio = { version = \"1\", features = [\"full\"] }\n");
        }
        s
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn ir_type_to_rust(ty: &IrType) -> String {
    match ty {
        IrType::Int     => "i64".to_string(),
        IrType::Bool    => "bool".to_string(),
        IrType::String  => "String".to_string(),
        IrType::Void    => "()".to_string(),
        IrType::List(t) => format!("Vec<{}>", ir_type_to_rust(t)),
        IrType::Fn(params, ret) => {
            let p = params.iter().map(ir_type_to_rust).collect::<Vec<_>>().join(", ");
            format!("impl Fn({}) -> {}", p, ir_type_to_rust(ret))
        }
        IrType::Named(n) => n.clone(),
        IrType::Unknown  => "_".to_string(),
    }
}

fn ir_binop_to_rust(op: &IrBinOp) -> &'static str {
    match op {
        IrBinOp::Add => "+",  IrBinOp::Sub => "-",
        IrBinOp::Mul => "*",  IrBinOp::Div => "/",
        IrBinOp::Mod => "%",  IrBinOp::Eq  => "==",
        IrBinOp::Ne  => "!=", IrBinOp::Lt  => "<",
        IrBinOp::Gt  => ">",  IrBinOp::Le  => "<=",
        IrBinOp::Ge  => ">=", IrBinOp::And => "&&",
        IrBinOp::Or  => "||",
    }
}

/// Returns `true` if the IR uses any known async stdlib types (HttpServer, Socket…).
fn uses_tokio(ir: &IrModule) -> bool {
    fn expr_uses_tokio(e: &IrExpr) -> bool {
        match e {
            IrExpr::Invoke { callee, .. } => {
                matches!(callee.as_str(), "HttpServer" | "Socket")
            }
            IrExpr::Call { receiver, args, .. } => {
                expr_uses_tokio(receiver) || args.iter().any(expr_uses_tokio)
            }
            IrExpr::Closure { body, .. } => expr_uses_tokio(body),
            IrExpr::Await(_) => true,
            _ => false,
        }
    }
    fn stmts_use_tokio(stmts: &[IrStmt]) -> bool {
        stmts.iter().any(|s| match s {
            IrStmt::Let { init, .. } => expr_uses_tokio(init),
            IrStmt::Discard(e) | IrStmt::Return(Some(e)) | IrStmt::Assign { value: e, .. } => expr_uses_tokio(e),
            IrStmt::If { cond, then_body, else_body } => {
                expr_uses_tokio(cond)
                    || stmts_use_tokio(then_body)
                    || else_body.as_deref().map(stmts_use_tokio).unwrap_or(false)
            }
            IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, .. } => {
                expr_uses_tokio(cond) || stmts_use_tokio(body)
            }
            _ => false,
        })
    }
    ir.classes.iter().any(|c| c.methods.iter().any(|m| stmts_use_tokio(&m.body)))
}

fn emit_class(out: &mut String, cls: &IrClass) -> Result<(), RustCodegenError> {
    writeln!(out, "struct {} {{", cls.name)?;
    for field in &cls.fields {
        writeln!(out, "    {}: {},", field.name, ir_type_to_rust(&field.ty))?;
    }
    writeln!(out, "}}")?;
    writeln!(out)?;
    Ok(())
}

fn emit_stmts(out: &mut String, stmts: &[IrStmt], indent: usize) -> Result<(), RustCodegenError> {
    let pad = "    ".repeat(indent);
    for stmt in stmts {
        emit_stmt(out, stmt, indent, &pad)?;
    }
    Ok(())
}

fn emit_stmt(
    out: &mut String,
    stmt: &IrStmt,
    indent: usize,
    pad: &str,
) -> Result<(), RustCodegenError> {
    match stmt {
        IrStmt::Let { name, ty, init } => {
            // Drop Console / stdlib marker instantiations silently.
            if let IrExpr::Invoke { callee, .. } = init {
                if is_stdlib_marker(callee) {
                    return Ok(());
                }
            }
            let rust_ty = ir_type_to_rust(ty);
            let rust_init = emit_expr(init)?;
            writeln!(out, "{pad}let {name}: {rust_ty} = {rust_init};")?;
        }

        IrStmt::Assign { target, value } => {
            writeln!(out, "{pad}{} = {};", emit_expr(target)?, emit_expr(value)?)?;
        }

        IrStmt::Return(Some(e)) => {
            // Suppress void returns (return;)
            if !matches!(e, IrExpr::Invoke { callee, .. } if callee == "Void") {
                writeln!(out, "{pad}return {};", emit_expr(e)?)?;
            }
        }
        IrStmt::Return(None) => {
            writeln!(out, "{pad}return;")?;
        }

        IrStmt::Discard(e) => {
            if let Some(line) = emit_expr_stmt(e)? {
                writeln!(out, "{pad}{line};")?;
            }
        }

        IrStmt::If { cond, then_body, else_body } => {
            writeln!(out, "{pad}if {} {{", emit_expr(cond)?)?;
            emit_stmts(out, then_body, indent + 1)?;
            if let Some(eb) = else_body {
                writeln!(out, "{pad}}} else {{")?;
                emit_stmts(out, eb, indent + 1)?;
            }
            writeln!(out, "{pad}}}")?;
        }

        IrStmt::While { cond, body } => {
            writeln!(out, "{pad}while {} {{", emit_expr(cond)?)?;
            emit_stmts(out, body, indent + 1)?;
            writeln!(out, "{pad}}}")?;
        }

        IrStmt::For { var, iter, body } => {
            writeln!(out, "{pad}for {var} in {} {{", emit_expr(iter)?)?;
            emit_stmts(out, body, indent + 1)?;
            writeln!(out, "{pad}}}")?;
        }

        IrStmt::Break => { writeln!(out, "{pad}break;")?; }
        IrStmt::Continue => { writeln!(out, "{pad}continue;")?; }

        IrStmt::Match { subject_var, subject, arms } => {
            writeln!(out, "{pad}let {subject_var} = {};", emit_expr(subject)?)?;
            writeln!(out, "{pad}match {subject_var} {{")?;
            let inner_pad = "    ".repeat(indent + 1);
            for arm in arms {
                if let Some(cond) = &arm.condition {
                    writeln!(out, "{inner_pad}{} => {{", emit_expr(cond)?)?;
                } else {
                    writeln!(out, "{inner_pad}_ => {{")?;
                }
                emit_stmts(out, &arm.body, indent + 2)?;
                writeln!(out, "{inner_pad}}}")?;
            }
            writeln!(out, "{pad}}}")?;
        }
    }
    Ok(())
}

/// Emit an expression as a *statement* line, handling stdlib calls specially.
/// Returns `None` if the expression should be silently dropped.
fn emit_expr_stmt(e: &IrExpr) -> Result<Option<String>, RustCodegenError> {
    match e {
        // Console.log / Console.info / etc. → println!
        IrExpr::Call { method, args, .. }
            if matches!(method.as_str(), "log" | "info" | "warn" | "error" | "debug") =>
        {
            let arg = args.first().map(|a| emit_expr(a)).transpose()?.unwrap_or_default();
            Ok(Some(format!("println!(\"{{}}\", {arg})")))
        }
        IrExpr::Await(inner) => Ok(Some(format!("{}.await", emit_expr(inner)?))),
        _ => Ok(Some(emit_expr(e)?)),
    }
}

fn emit_expr(e: &IrExpr) -> Result<String, RustCodegenError> {
    match e {
        IrExpr::Int(n) => Ok(n.to_string()),
        IrExpr::Bool(b) => Ok(b.to_string()),
        IrExpr::Str(s) => Ok(format!("\"{s}\".to_string()")),
        IrExpr::Local(n) => Ok(n.clone()),
        IrExpr::SelfRef => Ok("self".to_string()),

        IrExpr::Field { receiver, name } => {
            Ok(format!("{}.{name}", emit_expr(receiver)?))
        }

        IrExpr::Call { receiver, method, args } => {
            let recv = emit_expr(receiver)?;
            let a = args.iter().map(emit_expr).collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{recv}.{method}({})", a.join(", ")))
        }

        IrExpr::Invoke { callee, args } => {
            let a = args.iter().map(emit_expr).collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{callee}({})", a.join(", ")))
        }

        IrExpr::Bin { op, lhs, rhs } => {
            Ok(format!("({} {} {})", emit_expr(lhs)?, ir_binop_to_rust(op), emit_expr(rhs)?))
        }

        IrExpr::Unary { op, operand } => {
            let op_str = match op { IrUnOp::Not => "!", IrUnOp::Neg => "-" };
            Ok(format!("{op_str}{}", emit_expr(operand)?))
        }

        IrExpr::Await(inner) => Ok(format!("{}.await", emit_expr(inner)?)),

        IrExpr::List(items) => {
            let elems = items.iter().map(emit_expr).collect::<Result<Vec<_>, _>>()?;
            Ok(format!("vec![{}]", elems.join(", ")))
        }

        IrExpr::Closure { params, body } => {
            let ps = params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
            Ok(format!("|{ps}| {}", emit_expr(body)?))
        }

        IrExpr::Node { tag, .. } => Ok(format!("/* UI node: {tag} */")),
        IrExpr::DynamicImport(p) => Ok(format!("/* import(\"{p}\") */")),
    }
}

/// Returns true if `callee` is a stdlib type that should be dropped as a binding target.
fn is_stdlib_marker(callee: &str) -> bool {
    matches!(callee, "Console")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ir::*;

    fn make_gen() -> RustCodegen<'static> {
        RustCodegen::new("core", "my-app", "0.1.0")
    }

    fn make_ir(name: &str, stmts: Vec<IrStmt>) -> IrModule {
        IrModule {
            name: name.to_string(),
            server: None,
            enums: vec![],
            classes: vec![IrClass {
                name: name.to_string(),
                kind: IrClassKind::Class,
                is_public: false,
                fields: vec![],
                constructor_params: vec![],
                constructor_body: vec![],
                methods: vec![IrMethod {
                    name: "main".to_string(),
                    params: vec![],
                    return_ty: IrType::Void,
                    body: stmts,
                    is_public: false,
                    is_async: false,
                }],
            }],
            routes: vec![],
        }
    }

    #[test]
    fn generates_fn_main_for_empty_app() {
        let ir = make_ir("MyCli", vec![]);
        let result = make_gen().generate(&ir).unwrap();
        assert!(result.main_rs.contains("fn main()"), "should emit fn main()");
        assert!(!result.main_rs.contains("async"), "should not be async");
    }

    #[test]
    fn generates_tokio_main_for_async_app() {
        let ir = IrModule {
            name: "MyApi".to_string(),
            server: None,
            enums: vec![],
            classes: vec![IrClass {
                name: "MyApi".to_string(),
                kind: IrClassKind::Class,
                is_public: false,
                fields: vec![],
                constructor_params: vec![],
                constructor_body: vec![],
                methods: vec![IrMethod {
                    name: "main".to_string(),
                    params: vec![],
                    return_ty: IrType::Void,
                    body: vec![],
                    is_public: false,
                    is_async: true,
                }],
            }],
            routes: vec![],
        };
        let result = make_gen().generate(&ir).unwrap();
        assert!(result.main_rs.contains("#[tokio::main]"), "should emit #[tokio::main]");
        assert!(result.main_rs.contains("async fn main()"), "should emit async fn main()");
        assert!(result.cargo_toml.contains("tokio"), "Cargo.toml should include tokio");
    }

    #[test]
    fn let_int_binding_emits_i64() {
        let stmts = vec![IrStmt::Let {
            name: "x".to_string(),
            ty: IrType::Int,
            init: IrExpr::Int(42),
        }];
        let ir = make_ir("App", stmts);
        let result = make_gen().generate(&ir).unwrap();
        assert!(result.main_rs.contains("let x: i64 = 42;"), "got:\n{}", result.main_rs);
    }

    #[test]
    fn let_string_binding_emits_to_string() {
        let stmts = vec![IrStmt::Let {
            name: "s".to_string(),
            ty: IrType::String,
            init: IrExpr::Str("hello".to_string()),
        }];
        let ir = make_ir("App", stmts);
        let result = make_gen().generate(&ir).unwrap();
        assert!(
            result.main_rs.contains("let s: String = \"hello\".to_string();"),
            "got:\n{}",
            result.main_rs
        );
    }

    #[test]
    fn console_log_emits_println() {
        let stmts = vec![
            IrStmt::Let {
                name: "c".to_string(),
                ty: IrType::Named("Console".to_string()),
                init: IrExpr::Invoke { callee: "Console".to_string(), args: vec![] },
            },
            IrStmt::Discard(IrExpr::Call {
                receiver: Box::new(IrExpr::Local("c".to_string())),
                method: "log".to_string(),
                args: vec![IrExpr::Str("hello".to_string())],
            }),
        ];
        let ir = make_ir("App", stmts);
        let result = make_gen().generate(&ir).unwrap();
        assert!(
            result.main_rs.contains("println!("),
            "should emit println!, got:\n{}",
            result.main_rs
        );
        assert!(
            !result.main_rs.contains("let c: Console"),
            "Console binding should be dropped, got:\n{}",
            result.main_rs
        );
    }

    #[test]
    fn binary_op_add_emits_plus() {
        let stmts = vec![IrStmt::Let {
            name: "r".to_string(),
            ty: IrType::Int,
            init: IrExpr::Bin {
                op: IrBinOp::Add,
                lhs: Box::new(IrExpr::Int(1)),
                rhs: Box::new(IrExpr::Int(2)),
            },
        }];
        let ir = make_ir("App", stmts);
        let result = make_gen().generate(&ir).unwrap();
        assert!(result.main_rs.contains("(1 + 2)"), "got:\n{}", result.main_rs);
    }

    #[test]
    fn cargo_toml_contains_package_info() {
        let ir = make_ir("App", vec![]);
        let result = make_gen().generate(&ir).unwrap();
        assert!(result.cargo_toml.contains("[package]"));
        assert!(result.cargo_toml.contains("my-app-core"));
        assert!(result.cargo_toml.contains("0.1.0"));
        assert!(result.cargo_toml.contains("[[bin]]"));
    }

    #[test]
    fn error_when_no_app_class() {
        let ir = IrModule {
            name: "Ghost".to_string(),
            server: None,
            enums: vec![],
            classes: vec![],
            routes: vec![],
        };
        let err = make_gen().generate(&ir).unwrap_err();
        assert!(matches!(err, RustCodegenError::NoAppClass(_)));
    }

    #[test]
    fn error_when_no_main_method() {
        let ir = IrModule {
            name: "App".to_string(),
            server: None,
            enums: vec![],
            classes: vec![IrClass {
                name: "App".to_string(),
                kind: IrClassKind::Class,
                is_public: false,
                fields: vec![],
                constructor_params: vec![],
                constructor_body: vec![],
                methods: vec![],
            }],
            routes: vec![],
        };
        let err = make_gen().generate(&ir).unwrap_err();
        assert!(matches!(err, RustCodegenError::NoMainMethod(_)));
    }
}
