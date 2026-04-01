use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Name {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub eof: usize,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Local {
        names: Vec<Name>,
        values: Vec<Expr>,
    },
    Global {
        entries: Vec<GlobalEntry>,
    },
    Assign {
        targets: Vec<Expr>,
        values: Vec<Expr>,
    },
    FunctionDecl {
        name: FuncName,
        params: Vec<Name>,
        body: Vec<Stmt>,
        is_local: bool,
    },
    If {
        arms: Vec<(Expr, Vec<Stmt>, Span)>,
        else_body: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Repeat {
        body: Vec<Stmt>,
        cond: Expr,
    },
    NumericFor {
        var: Name,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },
    Return {
        values: Vec<Expr>,
    },
    Break,
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct GlobalEntry {
    pub name: Name,
    pub is_volatile: bool,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FuncName {
    pub parts: Vec<Name>,
    pub span: Span,
}

impl FuncName {
    pub fn as_string(&self) -> String {
        self.parts.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join(".")
    }
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Name(Name),
    Nil,
    Bool(bool),
    Number(String),
    String(String),
    Table(Vec<TableField>),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        lhs: Box<Expr>,
        op: BinaryOp,
        rhs: Box<Expr>,
    },
    Member {
        base: Box<Expr>,
        name: Name,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Function {
        params: Vec<Name>,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone)]
pub struct TableField {
    pub key: Option<Name>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl BinaryOp {
    fn binding_power(self) -> (u8, u8) {
        match self {
            BinaryOp::Or => (1, 2),
            BinaryOp::And => (3, 4),
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                (5, 6)
            }
            BinaryOp::Add | BinaryOp::Sub => (7, 8),
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => (9, 10),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Ident,
    Number,
    String,
    Comment,
    And,
    Break,
    Do,
    Else,
    ElseIf,
    End,
    False,
    For,
    Function,
    Global,
    If,
    In,
    Local,
    Nil,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    True,
    Until,
    Volatile,
    While,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Colon,
    Semi,
    Assign,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eof,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

pub fn lex(text: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(text);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        let end = token.kind == TokenKind::Eof;
        tokens.push(token);
        if end {
            break;
        }
    }
    tokens
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut iter = self.src[self.pos..].chars();
        iter.next()?;
        iter.next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn next_token(&mut self) -> Token {
        self.skip_ws();
        let start = self.pos;
        let Some(ch) = self.peek() else {
            return Token {
                kind: TokenKind::Eof,
                text: String::new(),
                span: Span { start, end: start },
            };
        };

        if ch == '-' && self.peek_next() == Some('-') {
            self.bump();
            self.bump();
            while let Some(c) = self.peek() {
                if c == '\n' {
                    break;
                }
                self.bump();
            }
            let end = self.pos;
            return Token {
                kind: TokenKind::Comment,
                text: self.src[start..end].to_string(),
                span: Span { start, end },
            };
        }

        if ch == '"' || ch == '\'' {
            let quote = ch;
            self.bump();
            let mut escaped = false;
            while let Some(c) = self.peek() {
                self.bump();
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' {
                    escaped = true;
                    continue;
                }
                if c == quote {
                    break;
                }
            }
            let end = self.pos;
            return Token {
                kind: TokenKind::String,
                text: self.src[start..end].to_string(),
                span: Span { start, end },
            };
        }

        if ch.is_ascii_digit() {
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '.' {
                    self.bump();
                } else {
                    break;
                }
            }
            let end = self.pos;
            return Token {
                kind: TokenKind::Number,
                text: self.src[start..end].to_string(),
                span: Span { start, end },
            };
        }

        if is_ident_start(ch) {
            self.bump();
            while let Some(c) = self.peek() {
                if is_ident_continue(c) {
                    self.bump();
                } else {
                    break;
                }
            }
            let end = self.pos;
            let text = &self.src[start..end];
            let kind = match text {
                "and" => TokenKind::And,
                "break" => TokenKind::Break,
                "do" => TokenKind::Do,
                "else" => TokenKind::Else,
                "elseif" => TokenKind::ElseIf,
                "end" => TokenKind::End,
                "false" => TokenKind::False,
                "for" => TokenKind::For,
                "function" => TokenKind::Function,
                "global" => TokenKind::Global,
                "if" => TokenKind::If,
                "in" => TokenKind::In,
                "local" => TokenKind::Local,
                "nil" => TokenKind::Nil,
                "not" => TokenKind::Not,
                "or" => TokenKind::Or,
                "repeat" => TokenKind::Repeat,
                "return" => TokenKind::Return,
                "then" => TokenKind::Then,
                "true" => TokenKind::True,
                "until" => TokenKind::Until,
                "volatile" => TokenKind::Volatile,
                "while" => TokenKind::While,
                _ => TokenKind::Ident,
            };
            return Token {
                kind,
                text: text.to_string(),
                span: Span { start, end },
            };
        }

        let (kind, len) = match (ch, self.peek_next()) {
            ('=', Some('=')) => (TokenKind::EqEq, 2),
            ('~', Some('=')) => (TokenKind::NotEq, 2),
            ('<', Some('=')) => (TokenKind::Le, 2),
            ('>', Some('=')) => (TokenKind::Ge, 2),
            _ => match ch {
                '(' => (TokenKind::LParen, 1),
                ')' => (TokenKind::RParen, 1),
                '{' => (TokenKind::LBrace, 1),
                '}' => (TokenKind::RBrace, 1),
                '[' => (TokenKind::LBracket, 1),
                ']' => (TokenKind::RBracket, 1),
                ',' => (TokenKind::Comma, 1),
                '.' => (TokenKind::Dot, 1),
                ':' => (TokenKind::Colon, 1),
                ';' => (TokenKind::Semi, 1),
                '=' => (TokenKind::Assign, 1),
                '<' => (TokenKind::Lt, 1),
                '>' => (TokenKind::Gt, 1),
                '+' => (TokenKind::Plus, 1),
                '-' => (TokenKind::Minus, 1),
                '*' => (TokenKind::Star, 1),
                '/' => (TokenKind::Slash, 1),
                '%' => (TokenKind::Percent, 1),
                _ => (TokenKind::Unknown, ch.len_utf8()),
            },
        };

        for _ in 0..len {
            self.bump();
        }
        let end = self.pos;
        Token {
            kind,
            text: self.src[start..end].to_string(),
            span: Span { start, end },
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

pub fn parse(text: &str) -> (Program, Vec<ParseError>, Vec<Token>) {
    let tokens = lex(text);
    let mut parser = Parser::new(tokens.clone());
    let program = parser.parse_program();
    (program, parser.errors, tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    fn parse_program(&mut self) -> Program {
        let mut stmts = Vec::new();
        while !self.at(TokenKind::Eof) {
            if self.at(TokenKind::Comment) {
                self.bump();
                continue;
            }
            match self.parse_stmt() {
                Some(stmt) => stmts.push(stmt),
                None => self.recover_stmt(),
            }
        }
        let eof = self.current().span.end;
        Program { stmts, eof }
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        match self.current().kind {
            TokenKind::Local => self.parse_local_or_local_function(),
            TokenKind::Global => self.parse_global(),
            TokenKind::Volatile => self.parse_volatile_global(),
            TokenKind::Function => self.parse_function_decl(false),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Repeat => self.parse_repeat(),
            TokenKind::For => self.parse_numeric_for(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Break => {
                let tok = self.bump();
                Some(Stmt {
                    kind: StmtKind::Break,
                    span: tok.span,
                })
            }
            _ => self.parse_assign_or_expr_stmt(),
        }
    }

    fn parse_local_or_local_function(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::Local)?;
        if self.at(TokenKind::Function) {
            return self.parse_function_decl_with_start(true, start.span.start);
        }
        let mut names = vec![self.parse_name()?];
        while self.eat(TokenKind::Comma).is_some() {
            names.push(self.parse_name()?);
        }
        let mut values = Vec::new();
        let end = if self.eat(TokenKind::Assign).is_some() {
            values = self.parse_expr_list()?;
            values.last().map(|x| x.span.end).unwrap_or(start.span.end)
        } else {
            names.last().map(|x| x.span.end).unwrap_or(start.span.end)
        };
        Some(Stmt {
            kind: StmtKind::Local { names, values },
            span: Span {
                start: start.span.start,
                end,
            },
        })
    }

    fn parse_global(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::Global)?;
        self.parse_global_entries(start.span.start, false)
    }

    fn parse_volatile_global(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::Volatile)?;
        self.expect(TokenKind::Global)?;
        self.parse_global_entries(start.span.start, true)
    }

    fn parse_global_entries(&mut self, start_offset: usize, is_volatile: bool) -> Option<Stmt> {
        let mut entries = Vec::new();
        loop {
            let name = self.parse_name()?;
            let mut end = name.span.end;
            let value = if self.eat(TokenKind::Assign).is_some() {
                let expr = self.parse_expr_bp(0)?;
                end = expr.span.end;
                Some(expr)
            } else {
                None
            };
            let span = Span {
                start: if entries.is_empty() { start_offset } else { name.span.start },
                end,
            };
            entries.push(GlobalEntry {
                name,
                is_volatile,
                value,
                span,
            });
            if self.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        let end = entries.last().map(|x| x.span.end).unwrap_or(start_offset);
        Some(Stmt {
            kind: StmtKind::Global { entries },
            span: Span {
                start: start_offset,
                end,
            },
        })
    }

    fn parse_function_decl(&mut self, is_local: bool) -> Option<Stmt> {
        self.parse_function_decl_with_start(is_local, self.current().span.start)
    }

    fn parse_function_decl_with_start(&mut self, is_local: bool, start_offset: usize) -> Option<Stmt> {
        self.expect(TokenKind::Function)?;
        let name = self.parse_func_name()?;
        let (params, body, end) = self.parse_function_body()?;
        Some(Stmt {
            kind: StmtKind::FunctionDecl {
                name,
                params,
                body,
                is_local,
            },
            span: Span {
                start: start_offset,
                end,
            },
        })
    }

    fn parse_if(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::If)?;
        let mut arms = Vec::new();
        let first_cond = self.parse_expr_bp(0)?;
        self.expect(TokenKind::Then)?;
        let (body, _, span) = self.parse_block_until(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::End]);
        arms.push((first_cond, body, span));

        while self.at(TokenKind::ElseIf) {
            self.expect(TokenKind::ElseIf)?;
            let cond = self.parse_expr_bp(0)?;
            self.expect(TokenKind::Then)?;
            let (body, _, span) = self.parse_block_until(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::End]);
            arms.push((cond, body, span));
        }

        let else_body = if self.at(TokenKind::Else) {
            self.expect(TokenKind::Else)?;
            let (body, _, _) = self.parse_block_until(&[TokenKind::End]);
            Some(body)
        } else {
            None
        };
        let end = self.expect(TokenKind::End)?.span.end;
        Some(Stmt {
            kind: StmtKind::If { arms, else_body },
            span: Span {
                start: start.span.start,
                end,
            },
        })
    }

    fn parse_while(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::While)?;
        let cond = self.parse_expr_bp(0)?;
        self.expect(TokenKind::Do)?;
        let (body, _, _) = self.parse_block_until(&[TokenKind::End]);
        let end = self.expect(TokenKind::End)?.span.end;
        Some(Stmt {
            kind: StmtKind::While { cond, body },
            span: Span {
                start: start.span.start,
                end,
            },
        })
    }

    fn parse_repeat(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::Repeat)?;
        let (body, _, _) = self.parse_block_until(&[TokenKind::Until]);
        self.expect(TokenKind::Until)?;
        let cond = self.parse_expr_bp(0)?;
        Some(Stmt {
            kind: StmtKind::Repeat { body, cond: cond.clone() },
            span: Span {
                start: start.span.start,
                end: cond.span.end,
            },
        })
    }

    fn parse_numeric_for(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::For)?;
        let var = self.parse_name()?;
        self.expect(TokenKind::Assign)?;
        let begin = self.parse_expr_bp(0)?;
        self.expect(TokenKind::Comma)?;
        let end_expr = self.parse_expr_bp(0)?;
        let step = if self.eat(TokenKind::Comma).is_some() {
            Some(self.parse_expr_bp(0)?)
        } else {
            None
        };
        self.expect(TokenKind::Do)?;
        let (body, _, _) = self.parse_block_until(&[TokenKind::End]);
        let end = self.expect(TokenKind::End)?.span.end;
        Some(Stmt {
            kind: StmtKind::NumericFor {
                var,
                start: begin,
                end: end_expr,
                step,
                body,
            },
            span: Span {
                start: start.span.start,
                end,
            },
        })
    }

    fn parse_return(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::Return)?;
        let values = match self.current().kind {
            TokenKind::End | TokenKind::Else | TokenKind::ElseIf | TokenKind::Until | TokenKind::Eof => Vec::new(),
            _ => self.parse_expr_list().unwrap_or_default(),
        };
        let end = values.last().map(|x| x.span.end).unwrap_or(start.span.end);
        Some(Stmt {
            kind: StmtKind::Return { values },
            span: Span {
                start: start.span.start,
                end,
            },
        })
    }

    fn parse_assign_or_expr_stmt(&mut self) -> Option<Stmt> {
        let first = self.parse_expr_bp(0)?;
        let mut exprs = vec![first];
        while self.eat(TokenKind::Comma).is_some() {
            exprs.push(self.parse_expr_bp(0)?);
        }
        if self.eat(TokenKind::Assign).is_some() {
            let values = self.parse_expr_list()?;
            let start = exprs.first().unwrap().span.start;
            let end = values.last().map(|x| x.span.end).unwrap_or(exprs.last().unwrap().span.end);
            return Some(Stmt {
                kind: StmtKind::Assign { targets: exprs, values },
                span: Span { start, end },
            });
        }
        if exprs.len() == 1 {
            let expr = exprs.into_iter().next().unwrap();
            let span = expr.span;
            return Some(Stmt {
                kind: StmtKind::Expr(expr),
                span,
            });
        }
        self.error_here("expected '=' after assignment target list");
        None
    }

    fn parse_expr_list(&mut self) -> Option<Vec<Expr>> {
        let mut exprs = vec![self.parse_expr_bp(0)?];
        while self.eat(TokenKind::Comma).is_some() {
            exprs.push(self.parse_expr_bp(0)?);
        }
        Some(exprs)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Expr> {
        let mut lhs = self.parse_prefix()?;
        loop {
            if self.at(TokenKind::Comment) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::Dot) {
                self.bump();
                let name = self.parse_name()?;
                let span = lhs.span.join(name.span);
                lhs = Expr {
                    kind: ExprKind::Member {
                        base: Box::new(lhs),
                        name,
                    },
                    span,
                };
                continue;
            }
            if self.at(TokenKind::LBracket) {
                let start = lhs.span.start;
                self.bump();
                let index = self.parse_expr_bp(0)?;
                let end = self.expect(TokenKind::RBracket)?.span.end;
                lhs = Expr {
                    kind: ExprKind::Index {
                        base: Box::new(lhs),
                        index: Box::new(index),
                    },
                    span: Span { start, end },
                };
                continue;
            }
            if self.at(TokenKind::LParen) {
                let start = lhs.span.start;
                let args = self.parse_call_args()?;
                let end = self.prev().span.end;
                lhs = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(lhs),
                        args,
                    },
                    span: Span { start, end },
                };
                continue;
            }
            let Some(op) = self.current_binary_op() else {
                break;
            };
            let (lbp, rbp) = op.binding_power();
            if lbp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr_bp(rbp)?;
            let span = lhs.span.join(rhs.span);
            lhs = Expr {
                kind: ExprKind::Binary {
                    lhs: Box::new(lhs),
                    op,
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(lhs)
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        if self.at(TokenKind::Comment) {
            self.bump();
        }
        let tok = self.current().clone();
        match tok.kind {
            TokenKind::Ident => {
                let name = self.parse_name()?;
                Some(Expr {
                    span: name.span,
                    kind: ExprKind::Name(name),
                })
            }
            TokenKind::Nil => {
                self.bump();
                Some(Expr { kind: ExprKind::Nil, span: tok.span })
            }
            TokenKind::True => {
                self.bump();
                Some(Expr { kind: ExprKind::Bool(true), span: tok.span })
            }
            TokenKind::False => {
                self.bump();
                Some(Expr { kind: ExprKind::Bool(false), span: tok.span })
            }
            TokenKind::Number => {
                self.bump();
                Some(Expr { kind: ExprKind::Number(tok.text), span: tok.span })
            }
            TokenKind::String => {
                self.bump();
                Some(Expr { kind: ExprKind::String(tok.text), span: tok.span })
            }
            TokenKind::Minus => {
                let start = self.bump().span.start;
                let expr = self.parse_expr_bp(11)?;
                Some(Expr {
                    span: Span { start, end: expr.span.end },
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::Not => {
                let start = self.bump().span.start;
                let expr = self.parse_expr_bp(11)?;
                Some(Expr {
                    span: Span { start, end: expr.span.end },
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_expr_bp(0)?;
                self.expect(TokenKind::RParen)?;
                Some(expr)
            }
            TokenKind::LBrace => self.parse_table(),
            TokenKind::Function => self.parse_function_expr(),
            _ => {
                self.error(tok.span, format!("unexpected token '{}' in expression", tok.text));
                None
            }
        }
    }

    fn parse_table(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::LBrace)?.span.start;
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            if self.at(TokenKind::Comment) {
                self.bump();
                continue;
            }
            let field_start = self.current().span.start;
            let key = if self.at(TokenKind::Ident) && self.peek_kind(1) == Some(TokenKind::Assign) {
                Some(self.parse_name()?)
            } else {
                None
            };
            if key.is_some() {
                self.expect(TokenKind::Assign)?;
            }
            let value = self.parse_expr_bp(0)?;
            let field_end = value.span.end;
            fields.push(TableField {
                key,
                value,
                span: Span {
                    start: field_start,
                    end: field_end,
                },
            });
            if self.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        let end = self.expect(TokenKind::RBrace)?.span.end;
        Some(Expr {
            kind: ExprKind::Table(fields),
            span: Span { start, end },
        })
    }

    fn parse_function_expr(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::Function)?.span.start;
        let (params, body, end) = self.parse_function_body()?;
        Some(Expr {
            kind: ExprKind::Function { params, body },
            span: Span { start, end },
        })
    }

    fn parse_function_body(&mut self) -> Option<(Vec<Name>, Vec<Stmt>, usize)> {
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.at(TokenKind::RParen) {
            params.push(self.parse_name()?);
            while self.eat(TokenKind::Comma).is_some() {
                params.push(self.parse_name()?);
            }
        }
        self.expect(TokenKind::RParen)?;
        let (body, _, _) = self.parse_block_until(&[TokenKind::End]);
        let end = self.expect(TokenKind::End)?.span.end;
        Some((params, body, end))
    }

    fn parse_call_args(&mut self) -> Option<Vec<Expr>> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if !self.at(TokenKind::RParen) {
            args.push(self.parse_expr_bp(0)?);
            while self.eat(TokenKind::Comma).is_some() {
                args.push(self.parse_expr_bp(0)?);
            }
        }
        self.expect(TokenKind::RParen)?;
        Some(args)
    }

    fn parse_func_name(&mut self) -> Option<FuncName> {
        let mut parts = vec![self.parse_name()?];
        while self.eat(TokenKind::Dot).is_some() {
            parts.push(self.parse_name()?);
        }
        let span = parts.first()?.span.join(parts.last()?.span);
        Some(FuncName { parts, span })
    }

    fn parse_name(&mut self) -> Option<Name> {
        let tok = self.expect(TokenKind::Ident)?;
        Some(Name {
            text: tok.text,
            span: tok.span,
        })
    }

    fn parse_block_until(&mut self, end_tokens: &[TokenKind]) -> (Vec<Stmt>, Option<TokenKind>, Span) {
        let mut stmts = Vec::new();
        while !self.at(TokenKind::Eof) && !end_tokens.contains(&self.current().kind) {
            if self.at(TokenKind::Comment) {
                self.bump();
                continue;
            }
            match self.parse_stmt() {
                Some(stmt) => stmts.push(stmt),
                None => self.recover_stmt(),
            }
        }
        let span = if let (Some(first), Some(last)) = (stmts.first(), stmts.last()) {
            first.span.join(last.span)
        } else {
            self.current().span
        };
        (stmts, Some(self.current().kind), span)
    }

    fn current_binary_op(&self) -> Option<BinaryOp> {
        match self.current().kind {
            TokenKind::Or => Some(BinaryOp::Or),
            TokenKind::And => Some(BinaryOp::And),
            TokenKind::EqEq => Some(BinaryOp::Eq),
            TokenKind::NotEq => Some(BinaryOp::Ne),
            TokenKind::Lt => Some(BinaryOp::Lt),
            TokenKind::Le => Some(BinaryOp::Le),
            TokenKind::Gt => Some(BinaryOp::Gt),
            TokenKind::Ge => Some(BinaryOp::Ge),
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Sub),
            TokenKind::Star => Some(BinaryOp::Mul),
            TokenKind::Slash => Some(BinaryOp::Div),
            TokenKind::Percent => Some(BinaryOp::Mod),
            _ => None,
        }
    }

    fn recover_stmt(&mut self) {
        while !self.at(TokenKind::Eof) {
            match self.current().kind {
                TokenKind::End | TokenKind::Else | TokenKind::ElseIf | TokenKind::Until => break,
                TokenKind::Comment => {
                    self.bump();
                }
                _ => {
                    self.bump();
                    if self.at(TokenKind::Comment) {
                        break;
                    }
                }
            }
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if self.current().kind == kind {
            Some(self.bump())
        } else {
            let current = self.current().clone();
            self.error(
                current.span,
                format!("expected {}, found '{}'", DisplayTokenKind(kind), current.text),
            );
            None
        }
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Token> {
        if self.current().kind == kind {
            Some(self.bump())
        } else {
            None
        }
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn prev(&self) -> &Token {
        if self.pos == 0 {
            &self.tokens[0]
        } else {
            &self.tokens[self.pos - 1]
        }
    }

    fn peek_kind(&self, lookahead: usize) -> Option<TokenKind> {
        self.tokens.get(self.pos + lookahead).map(|t| t.kind)
    }

    fn bump(&mut self) -> Token {
        let tok = self.current().clone();
        self.pos = (self.pos + 1).min(self.tokens.len().saturating_sub(1));
        tok
    }

    fn error(&mut self, span: Span, message: String) {
        self.errors.push(ParseError { message, span });
    }

    fn error_here(&mut self, message: &str) {
        self.error(self.current().span, message.to_string());
    }
}

struct DisplayTokenKind(TokenKind);

impl fmt::Display for DisplayTokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.0 {
            TokenKind::Ident => "identifier",
            TokenKind::Number => "number",
            TokenKind::String => "string",
            TokenKind::Comment => "comment",
            TokenKind::And => "and",
            TokenKind::Break => "break",
            TokenKind::Do => "do",
            TokenKind::Else => "else",
            TokenKind::ElseIf => "elseif",
            TokenKind::End => "end",
            TokenKind::False => "false",
            TokenKind::For => "for",
            TokenKind::Function => "function",
            TokenKind::Global => "global",
            TokenKind::If => "if",
            TokenKind::In => "in",
            TokenKind::Local => "local",
            TokenKind::Nil => "nil",
            TokenKind::Not => "not",
            TokenKind::Or => "or",
            TokenKind::Repeat => "repeat",
            TokenKind::Return => "return",
            TokenKind::Then => "then",
            TokenKind::True => "true",
            TokenKind::Until => "until",
            TokenKind::Volatile => "volatile",
            TokenKind::While => "while",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::Comma => ",",
            TokenKind::Dot => ".",
            TokenKind::Colon => ":",
            TokenKind::Semi => ";",
            TokenKind::Assign => "=",
            TokenKind::EqEq => "==",
            TokenKind::NotEq => "~=",
            TokenKind::Lt => "<",
            TokenKind::Le => "<=",
            TokenKind::Gt => ">",
            TokenKind::Ge => ">=",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::Eof => "<eof>",
            TokenKind::Unknown => "<unknown>",
        };
        write!(f, "{s}")
    }
}
