use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // ── App-level keywords ──────────────────────────────────────────────────
    App,
    Server,
    Route,
    Package,
    Import,
    // ── OO keywords ────────────────────────────────────────────────────────
    Class,
    Interface,
    Component,
    Window,
    Public,
    Private,
    Extends,
    Implements,
    Constructor,
    Return,
    This,
    // ── Control flow ───────────────────────────────────────────────────────
    If,
    Else,
    For,
    While,
    Break,
    Continue,
    Let,
    In,
    // ── Built-in types ─────────────────────────────────────────────────────
    TInt,
    TString,
    TBool,
    TVoid,
    TList,
    // ── Symbols ────────────────────────────────────────────────────────────
    LBrace,
    RBrace,
    LParen,
    RParen,
    LAngle,       // <
    RAngle,       // >
    FatArrow,     // =>
    Semicolon,
    Colon,
    Comma,
    Dot,
    // ── Assignment / comparison ────────────────────────────────────────────
    Equals,       // =
    EqualEqual,   // ==
    BangEqual,    // !=
    LessEqual,    // <=
    GreaterEqual, // >=
    // ── Arithmetic ─────────────────────────────────────────────────────────
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    // ── Logic ──────────────────────────────────────────────────────────────
    And,          // &&
    Or,           // ||
    Bang,         // !
    // ── Literals ───────────────────────────────────────────────────────────
    StringLit(String),
    IntLit(i64),
    BoolLit(bool),
    // ── Identifier / EOF ───────────────────────────────────────────────────
    Ident(String),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Error)]
pub enum LexError {
    #[error("Unexpected character '{0}' at line {1}, col {2}")]
    UnexpectedChar(char, usize, usize),
    #[error("Unterminated string at line {0}")]
    UnterminatedString(usize),
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer { chars: source.chars().collect(), pos: 0, line: 1, col: 1 }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if let Some(ch) = c {
            self.pos += 1;
            if ch == '\n' { self.line += 1; self.col = 1; } else { self.col += 1; }
        }
        c
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while self.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
                self.advance();
            }
            if self.peek() == Some('/') && self.peek2() == Some('/') {
                while self.peek().map(|c| c != '\n').unwrap_or(false) {
                    self.advance();
                }
                continue;
            }
            break;
        }
    }

    fn read_string(&mut self) -> Result<Token, LexError> {
        let line = self.line;
        self.advance(); // consume "
        let mut s = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => return Err(LexError::UnterminatedString(line)),
                Some('"') => { self.advance(); return Ok(Token::StringLit(s)); }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n')  => s.push('\n'),
                        Some('t')  => s.push('\t'),
                        Some('"')  => s.push('"'),
                        Some('\\') => s.push('\\'),
                        _ => {}
                    }
                }
                Some(c) => { s.push(c); self.advance(); }
            }
        }
    }

    fn read_number(&mut self) -> Token {
        let mut n = String::new();
        while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            n.push(self.advance().unwrap());
        }
        Token::IntLit(n.parse().unwrap_or(0))
    }

    fn read_ident_or_keyword(&mut self) -> Token {
        let mut s = String::new();
        while self.peek().map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false) {
            s.push(self.advance().unwrap());
        }
        match s.as_str() {
            "app"         => Token::App,
            "server"      => Token::Server,
            "route"       => Token::Route,
            "package"     => Token::Package,
            "import"      => Token::Import,
            "class"       => Token::Class,
            "interface"   => Token::Interface,
            "component"   => Token::Component,
            "window"      => Token::Window,
            "public"      => Token::Public,
            "private"     => Token::Private,
            "extends"     => Token::Extends,
            "implements"  => Token::Implements,
            "constructor" => Token::Constructor,
            "return"      => Token::Return,
            "this"        => Token::This,
            "if"          => Token::If,
            "else"        => Token::Else,
            "for"         => Token::For,
            "while"       => Token::While,
            "break"       => Token::Break,
            "continue"    => Token::Continue,
            "let"         => Token::Let,
            "in"          => Token::In,
            "Int"         => Token::TInt,
            "String"      => Token::TString,
            "Bool"        => Token::TBool,
            "Void"        => Token::TVoid,
            "List"        => Token::TList,
            "true"        => Token::BoolLit(true),
            "false"       => Token::BoolLit(false),
            _             => Token::Ident(s),
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Spanned>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let line = self.line;
            let col = self.col;

            match self.peek() {
                None => { tokens.push(Spanned { token: Token::Eof, line, col }); break; }
                Some('"') => {
                    let tok = self.read_string()?;
                    tokens.push(Spanned { token: tok, line, col });
                }
                Some(c) if c.is_ascii_digit() => {
                    let tok = self.read_number();
                    tokens.push(Spanned { token: tok, line, col });
                }
                Some(c) if c.is_alphabetic() || c == '_' => {
                    let tok = self.read_ident_or_keyword();
                    tokens.push(Spanned { token: tok, line, col });
                }
                Some('{') => { self.advance(); tokens.push(Spanned { token: Token::LBrace,     line, col }); }
                Some('}') => { self.advance(); tokens.push(Spanned { token: Token::RBrace,     line, col }); }
                Some('(') => { self.advance(); tokens.push(Spanned { token: Token::LParen,     line, col }); }
                Some(')') => { self.advance(); tokens.push(Spanned { token: Token::RParen,     line, col }); }
                Some(';') => { self.advance(); tokens.push(Spanned { token: Token::Semicolon,  line, col }); }
                Some(':') => { self.advance(); tokens.push(Spanned { token: Token::Colon,      line, col }); }
                Some(',') => { self.advance(); tokens.push(Spanned { token: Token::Comma,      line, col }); }
                Some('.') => { self.advance(); tokens.push(Spanned { token: Token::Dot,        line, col }); }
                Some('+') => { self.advance(); tokens.push(Spanned { token: Token::Plus,       line, col }); }
                Some('-') => { self.advance(); tokens.push(Spanned { token: Token::Minus,      line, col }); }
                Some('*') => { self.advance(); tokens.push(Spanned { token: Token::Star,       line, col }); }
                Some('%') => { self.advance(); tokens.push(Spanned { token: Token::Percent,    line, col }); }
                Some('=') => {
                    self.advance();
                    if self.peek() == Some('>') { self.advance(); tokens.push(Spanned { token: Token::FatArrow, line, col }); }
                    else if self.peek() == Some('=') { self.advance(); tokens.push(Spanned { token: Token::EqualEqual, line, col }); }
                    else { tokens.push(Spanned { token: Token::Equals, line, col }); }
                }
                Some('!') => {
                    self.advance();
                    if self.peek() == Some('=') { self.advance(); tokens.push(Spanned { token: Token::BangEqual, line, col }); }
                    else { tokens.push(Spanned { token: Token::Bang, line, col }); }
                }
                Some('<') => {
                    self.advance();
                    if self.peek() == Some('=') { self.advance(); tokens.push(Spanned { token: Token::LessEqual, line, col }); }
                    else { tokens.push(Spanned { token: Token::LAngle, line, col }); }
                }
                Some('>') => {
                    self.advance();
                    if self.peek() == Some('=') { self.advance(); tokens.push(Spanned { token: Token::GreaterEqual, line, col }); }
                    else { tokens.push(Spanned { token: Token::RAngle, line, col }); }
                }
                Some('&') => {
                    self.advance();
                    if self.peek() == Some('&') { self.advance(); tokens.push(Spanned { token: Token::And, line, col }); }
                    else { return Err(LexError::UnexpectedChar('&', line, col)); }
                }
                Some('|') => {
                    self.advance();
                    if self.peek() == Some('|') { self.advance(); tokens.push(Spanned { token: Token::Or, line, col }); }
                    else { return Err(LexError::UnexpectedChar('|', line, col)); }
                }
                Some('/') => {
                    self.advance();
                    // single slash as division operator (comments already handled above)
                    tokens.push(Spanned { token: Token::Slash, line, col });
                }
                Some(c) => return Err(LexError::UnexpectedChar(c, line, col)),
            }
        }
        Ok(tokens)
    }
}
