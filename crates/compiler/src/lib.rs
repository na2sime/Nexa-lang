pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod parser;
pub mod resolver;
pub mod semantic;
pub mod types;

pub use codegen::CodeGenerator;
pub use parser::Parser;
pub use resolver::Resolver;
pub use semantic::SemanticAnalyzer;

use std::path::Path;

#[derive(Debug)]
pub struct CompileResult {
    pub html: String,
    pub js: String,
}

/// Pipeline commun : lex → parse → resolve → semantic → codegen.
/// `entry` est utilisé pour résoudre les imports relatifs au fichier source.
/// `resolver_root` est la racine de recherche pour les imports de packages.
fn run_pipeline(
    source: &str,
    entry: &Path,
    resolver_root: &Path,
) -> Result<CompileResult, Box<dyn std::error::Error>> {
    let tokens = lexer::Lexer::new(source).tokenize()?;
    let program = parser::Parser::new(tokens).parse()?;
    let resolved = resolver::Resolver::new(resolver_root).resolve(&program, entry)?;
    let mut analyzer = semantic::SemanticAnalyzer::new();
    analyzer.analyze(&resolved)?;
    Ok(codegen::CodeGenerator::new().generate(&resolved)?)
}

/// Compile un fichier `.nexa` standalone, en résolvant les imports
/// relativement à son répertoire parent.
pub fn compile_file(path: &Path) -> Result<CompileResult, Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(path)?;
    let root = path.parent().unwrap_or(Path::new("."));
    run_pipeline(&source, path, root)
}

/// Compile un fichier `.nexa` dans le contexte d'un projet structuré.
/// `src_root` = `<project>/src/` — racine du Resolver, permet de résoudre
/// `libs/` en plus de `main/`.
pub fn compile_project_file(
    entry: &Path,
    src_root: &Path,
) -> Result<CompileResult, Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(entry)?;
    run_pipeline(&source, entry, src_root)
}

/// Compile depuis une string (sans résolution d'imports).
pub fn compile_str(source: &str) -> Result<CompileResult, Box<dyn std::error::Error>> {
    // Pour compile_str, il n'y a pas de fichier réel : on utilise un chemin fictif
    // et un resolver root vide. Les imports ne sont pas supportés.
    let tokens = lexer::Lexer::new(source).tokenize()?;
    let program = parser::Parser::new(tokens).parse()?;
    let mut analyzer = semantic::SemanticAnalyzer::new();
    analyzer.analyze(&program)?;
    Ok(codegen::CodeGenerator::new().generate(&program)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_APP: &str = r#"app App {
  server { port: 3000; }
  public window HomePage {
    public render() => Component {
      return Page { Text("Hi") };
    }
  }
  route "/" => HomePage;
}"#;

    #[test]
    fn compile_str_produces_html_and_js() {
        let result = compile_str(MINIMAL_APP).unwrap();
        assert!(!result.html.is_empty(), "html ne doit pas être vide");
        assert!(!result.js.is_empty(),   "js ne doit pas être vide");
    }

    #[test]
    fn compile_str_html_is_valid_document() {
        let result = compile_str(MINIMAL_APP).unwrap();
        assert!(result.html.contains("<!DOCTYPE html>"),  "html doit contenir <!DOCTYPE html>");
        assert!(result.html.contains(r#"id="app""#),      "html doit contenir div#app");
        assert!(result.html.contains("app.js"),           "html doit charger app.js");
    }

    #[test]
    fn compile_str_js_contains_window_class() {
        let result = compile_str(MINIMAL_APP).unwrap();
        assert!(result.js.contains("HomePage"), "js doit contenir la classe HomePage");
    }

    #[test]
    fn compile_str_js_contains_route() {
        let result = compile_str(MINIMAL_APP).unwrap();
        assert!(result.js.contains(r#"_routes["/"]"#), "js doit contenir la route /");
    }

    #[test]
    fn compile_str_syntax_error_returns_err() {
        assert!(compile_str("app { SYNTAXE INVALIDE !!! }").is_err());
    }

    #[test]
    fn compile_str_empty_source_returns_err() {
        assert!(compile_str("").is_err());
    }
}
