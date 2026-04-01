# Contributing to Nexa

Thank you for your interest in contributing! This document covers everything you need to get started.

## Table of Contents

- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Running Tests](#running-tests)
- [Making Changes](#making-changes)
- [Pull Request Process](#pull-request-process)
- [Code Style](#code-style)

---

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- Git

### Clone and build

```bash
git clone https://github.com/nassime/Nexa-lang.git
cd Nexa-lang
cargo build
```

### Install the CLI locally

```bash
cargo install --path crates/cli
```

---

## Project Structure

```
crates/
├── compiler/   Core language implementation
│   └── src/
│       ├── ast.rs       AST node types
│       ├── lexer.rs     Tokenizer
│       ├── parser.rs    Token → AST
│       ├── resolver.rs  Import resolution
│       ├── semantic.rs  Type checking
│       ├── codegen.rs   HTML + JS emission
│       └── lib.rs       Public API
├── cli/        Command-line interface
│   └── src/
│       ├── main.rs      CLI commands (run, build)
│       └── project.rs   project.json / nexa-compiler.yaml loading
└── server/     Axum dev server
```

---

## Running Tests

```bash
# All tests
cargo test

# Compiler only
cargo test -p nexa-compiler

# CLI only
cargo test -p nexa
```

To test against the example project manually:

```bash
cargo run --bin nexa -- build --project examples/
```

---

## Making Changes

### Compiler changes

- **New syntax** — update `lexer.rs` (tokens) → `parser.rs` (grammar) → `ast.rs` (nodes) → `semantic.rs` (validation) → `codegen.rs` (output)
- **New built-in** — add the primitive to `codegen.rs` `RUNTIME` constant and update the parser
- **New error variant** — add to the relevant error enum, with a clear human-readable message

### CLI changes

- **New command** — add a variant to `Commands` in `main.rs`, implement a handler function
- **Project config fields** — add to `ProjectConfig` in `project.rs` and update tests

### Tests

Every non-trivial change should include tests:
- Compiler logic → `crates/compiler/src/lib.rs` `#[cfg(test)]`
- Project loading → `crates/cli/src/project.rs` `#[cfg(test)]`

---

## Pull Request Process

1. **Fork** the repository and create a branch from `main`
2. **Make your changes** with focused commits
3. **Add tests** for new behaviour
4. **Run the full test suite** — `cargo test` must pass
5. **Run `cargo clippy`** — fix any warnings
6. **Open a PR** with a clear description of what and why

### Commit style

```
feat: add watch mode to nexa run
fix: resolver cycle detection on Windows paths
docs: update README quick start section
test: add coverage for missing entry file error
refactor: extract run_pipeline in compiler lib
```

---

## Code Style

- Follow standard Rust idioms (`cargo clippy` is the authority)
- Public types and functions should have doc comments
- Keep functions focused — if a function does IO, parsing, *and* side-effects, split it
- Prefer typed errors (`thiserror`) over `Box<dyn Error>` in new code
- Tests go in `#[cfg(test)]` modules at the bottom of the relevant file

---

## Questions?

Open a [GitHub Discussion](https://github.com/nassime/Nexa-lang/discussions) or a [GitHub Issue](https://github.com/nassime/Nexa-lang/issues).
