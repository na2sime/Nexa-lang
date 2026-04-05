//! Criterion benchmarks for the Nexa compiler pipeline.
//!
//! Run with:
//!   cargo bench -p nexa-compiler
//!   cargo bench -p nexa-compiler -- lexer    # filter by name

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nexa_compiler::application::services::{
    codegen::CodeGenerator,
    lexer::Lexer,
    lower::lower,
    optimizer::optimize,
    parser::Parser,
    semantic::SemanticAnalyzer,
    wasm_codegen::WasmCodegen,
};

// ── Benchmark inputs ──────────────────────────────────────────────────────────

/// A representative single-class Nexa program used for all pipeline stages.
const CLASS_SRC: &str = r#"
class Point {
    let x: Int;
    let y: Int;

    constructor(x: Int, y: Int) {
        self.x = x;
        self.y = y;
    }

    distance(other: Point) -> Int {
        let dx: Int = self.x - other.x;
        let dy: Int = self.y - other.y;
        return dx * dx + dy * dy;
    }

    scale(factor: Int) -> Point {
        return Point(self.x * factor, self.y * factor);
    }
}
"#;

/// A larger program that exercises the full app pipeline (routing, windows, etc.).
const APP_SRC: &str = r#"app MyApp {
    server { port: 3000; }

    class Counter {
        let count: Int;
        constructor() { self.count = 0; }
        increment() { self.count = self.count + 1; }
        decrement() { self.count = self.count - 1; }
        value() -> Int { return self.count; }
    }

    class Calculator {
        let result: Int;
        constructor() { self.result = 0; }
        add(n: Int) { self.result = self.result + n; }
        sub(n: Int) { self.result = self.result - n; }
        mul(n: Int) { self.result = self.result * n; }
        reset() { self.result = 0; }
        value() -> Int { return self.result; }
    }

    public window HomePage {
        public render() => Component {
            return Page {
                Text("Home")
            };
        }
    }

    public window AboutPage {
        public render() => Component {
            return Page {
                Text("About")
            };
        }
    }

    route "/" => HomePage;
    route "/about" => AboutPage;
}"#;

// ── Individual stage benchmarks ───────────────────────────────────────────────

fn bench_lexer(c: &mut Criterion) {
    c.bench_function("lexer/class_src", |b| {
        b.iter(|| {
            Lexer::new(black_box(CLASS_SRC))
                .tokenize()
                .expect("lex error")
        })
    });

    c.bench_function("lexer/app_src", |b| {
        b.iter(|| {
            Lexer::new(black_box(APP_SRC))
                .tokenize()
                .expect("lex error")
        })
    });
}

fn bench_parser(c: &mut Criterion) {
    // Pre-tokenise outside the timed region.
    let tokens_class = Lexer::new(CLASS_SRC).tokenize().expect("lex error");
    let tokens_app = Lexer::new(APP_SRC).tokenize().expect("lex error");

    c.bench_function("parser/class_src", |b| {
        b.iter(|| {
            Parser::new(black_box(tokens_class.clone()))
                .parse_lib()
                .expect("parse error")
        })
    });

    c.bench_function("parser/app_src", |b| {
        b.iter(|| {
            Parser::new(black_box(tokens_app.clone()))
                .parse()
                .expect("parse error")
        })
    });
}

fn bench_semantic(c: &mut Criterion) {
    let tokens = Lexer::new(CLASS_SRC).tokenize().expect("lex error");
    let program = Parser::new(tokens).parse_lib().expect("parse error");

    c.bench_function("semantic/class_src", |b| {
        b.iter(|| {
            let mut sa = SemanticAnalyzer::new();
            sa.analyze(black_box(&program)).expect("semantic error")
        })
    });

    let tokens_app = Lexer::new(APP_SRC).tokenize().expect("lex error");
    let program_app = Parser::new(tokens_app).parse().expect("parse error");

    c.bench_function("semantic/app_src", |b| {
        b.iter(|| {
            let mut sa = SemanticAnalyzer::new();
            sa.analyze(black_box(&program_app)).expect("semantic error")
        })
    });
}

fn bench_codegen_js(c: &mut Criterion) {
    let tokens = Lexer::new(APP_SRC).tokenize().expect("lex error");
    let program = Parser::new(tokens).parse().expect("parse error");
    let mut sa = SemanticAnalyzer::new();
    sa.analyze(&program).expect("semantic error");

    c.bench_function("codegen_js/app_src", |b| {
        b.iter(|| {
            CodeGenerator::new()
                .generate(black_box(&program))
                .expect("codegen error")
        })
    });
}

fn bench_codegen_wasm(c: &mut Criterion) {
    let tokens = Lexer::new(CLASS_SRC).tokenize().expect("lex error");
    let program = Parser::new(tokens).parse_lib().expect("parse error");
    let mut sa = SemanticAnalyzer::new();
    sa.analyze(&program).expect("semantic error");
    let optimized = optimize(program.clone());
    let ir = lower(&optimized);

    c.bench_function("codegen_wasm/class_src", |b| {
        b.iter(|| {
            WasmCodegen::new()
                .generate_wat(black_box(&ir))
                .expect("wasm codegen error")
        })
    });
}

fn bench_full_pipeline(c: &mut Criterion) {
    c.bench_function("pipeline/full_app_js", |b| {
        b.iter(|| nexa_compiler::compile_str(black_box(APP_SRC)).expect("compile error"))
    });
}

// ── Criterion groups ──────────────────────────────────────────────────────────

criterion_group!(
    benches_lexer,
    bench_lexer
);
criterion_group!(
    benches_parser,
    bench_parser
);
criterion_group!(
    benches_semantic,
    bench_semantic
);
criterion_group!(
    benches_codegen,
    bench_codegen_js,
    bench_codegen_wasm
);
criterion_group!(
    benches_pipeline,
    bench_full_pipeline
);

criterion_main!(
    benches_lexer,
    benches_parser,
    benches_semantic,
    benches_codegen,
    benches_pipeline
);
