use crate::ir::{Item, Label, OpKind};
use crate::lua::{Function, GlobalDecl, GlobalKind, Program, Stmt};
use crate::meta::Meta;
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CondKind {
    NonZero,
    Zero,
    AlwaysTrue,
    AlwaysFalse,
    Generic,
}

fn parse_cond(cond: &str) -> CondKind {
    let c = cond.trim();
    if c == "true" {
        return CondKind::AlwaysTrue;
    }
    if c == "false" || c == "nil" {
        return CondKind::AlwaysFalse;
    }

    let c = c.trim_start_matches('(').trim_end_matches(')').trim();

    let re_ne0 = Regex::new(r"^S\d+\s*~=\s*0$").unwrap();
    let re_eq0 = Regex::new(r"^S\d+\s*==\s*0$").unwrap();
    let re_s = Regex::new(r"^S\d+$").unwrap();

    if re_ne0.is_match(c) {
        return CondKind::NonZero;
    }
    if re_eq0.is_match(c) {
        return CondKind::Zero;
    }
    if re_s.is_match(c) {
        return CondKind::NonZero;
    }

    CondKind::Generic
}

#[derive(Clone, Debug)]
pub struct GlobalLayout {
    pub non_volatile_count: u16,
    pub volatile_count: u16,
    name_to_idx: HashMap<String, u16>,
    declared: HashSet<String>,
}

impl GlobalLayout {
    fn from_globals(globals: &[GlobalDecl]) -> Result<Self> {
        let mut max_g: Option<u16> = None;
        let mut max_vg: Option<u16> = None;
        let re_g = Regex::new(r"^g(\d+)$").unwrap();
        let re_vg = Regex::new(r"^vg(\d+)$").unwrap();
        let mut declared = HashSet::new();

        for g in globals {
            if !declared.insert(g.name.clone()) {
                bail!("duplicate global declaration: {}", g.name);
            }
            match g.kind {
                GlobalKind::NonVolatile => {
                    let caps = re_g
                        .captures(&g.name)
                        .ok_or_else(|| anyhow!("invalid non-volatile global name: {}", g.name))?;
                    let idx: u16 = caps.get(1).unwrap().as_str().parse()?;
                    max_g = Some(max_g.map(|x| x.max(idx)).unwrap_or(idx));
                }
                GlobalKind::Volatile => {
                    let caps = re_vg
                        .captures(&g.name)
                        .ok_or_else(|| anyhow!("invalid volatile global name: {}", g.name))?;
                    let idx: u16 = caps.get(1).unwrap().as_str().parse()?;
                    max_vg = Some(max_vg.map(|x| x.max(idx)).unwrap_or(idx));
                }
            }
        }

        let non_volatile_count = max_g.map(|x| x + 1).unwrap_or(0);
        let volatile_count = max_vg.map(|x| x + 1).unwrap_or(0);
        let mut name_to_idx = HashMap::new();
        for g in globals {
            let idx = match g.kind {
                GlobalKind::NonVolatile => g.name[1..].parse::<u16>()?,
                GlobalKind::Volatile => non_volatile_count + g.name[2..].parse::<u16>()?,
            };
            name_to_idx.insert(g.name.clone(), idx);
        }

        Ok(Self {
            non_volatile_count,
            volatile_count,
            name_to_idx,
            declared,
        })
    }

    fn global_idx(&self, name: &str) -> Option<u16> {
        self.name_to_idx.get(name).copied()
    }

    fn is_declared(&self, name: &str) -> bool {
        self.declared.contains(name)
    }
}

fn slot_to_stack_idx(var: &str, args_count: i8) -> Result<i8> {
    let re_a = Regex::new(r"^a(\d+)$").unwrap();
    let re_l = Regex::new(r"^l(\d+)$").unwrap();

    if let Some(c) = re_a.captures(var) {
        let v: i16 = c.get(1).unwrap().as_str().parse()?;
        return Ok(i8::try_from(v).map_err(|_| anyhow!("stack index out of i8"))?);
    }
    if let Some(c) = re_l.captures(var) {
        let v: i16 = c.get(1).unwrap().as_str().parse()?;
        let idx: i16 = i16::from(args_count) + v;
        return Ok(i8::try_from(idx).map_err(|_| anyhow!("stack index out of i8"))?);
    }
    bail!("not a frame slot: {var}")
}

fn push_int(v: i64) -> Result<OpKind> {
    if v < i64::from(i32::MIN) || v > i64::from(i32::MAX) {
        bail!("integer out of i32 range: {v}");
    }
    if v >= -128 && v <= 127 {
        return Ok(OpKind::PushI8(v as i8));
    }
    if v >= -32768 && v <= 32767 {
        return Ok(OpKind::PushI16(v as i16));
    }
    Ok(OpKind::PushI32(v as i32))
}

fn lua_unescape_string(lit: &str) -> String {
    let mut out = String::new();
    let mut chars = lit.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => out.push(other),
            None => break,
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Int(i64),
    Float(f32),
    Str(String),
    Nil,
    True,
    False,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

fn tokenize_expr(s: &str) -> Result<Vec<Tok>> {
    let mut toks = Vec::new();
    let b = s.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        let ch = b[i] as char;
        if ch.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        match ch {
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            '[' => {
                toks.push(Tok::LBracket);
                i += 1;
            }
            ']' => {
                toks.push(Tok::RBracket);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            '+' => {
                toks.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                if i + 1 < b.len() && (b[i + 1] as char).is_ascii_digit() {
                    let start = i;
                    i += 1;
                    while i < b.len() && (b[i] as char).is_ascii_digit() {
                        i += 1;
                    }
                    let mut is_float = false;
                    if i < b.len() && b[i] as char == '.' {
                        is_float = true;
                        i += 1;
                        while i < b.len() && (b[i] as char).is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let lit = &s[start..i];
                    if is_float {
                        toks.push(Tok::Float(lit.parse()?));
                    } else {
                        toks.push(Tok::Int(lit.parse()?));
                    }
                } else {
                    toks.push(Tok::Minus);
                    i += 1;
                }
            }
            '*' => {
                toks.push(Tok::Star);
                i += 1;
            }
            '/' => {
                toks.push(Tok::Slash);
                i += 1;
            }
            '%' => {
                toks.push(Tok::Percent);
                i += 1;
            }
            '&' => {
                toks.push(Tok::Amp);
                i += 1;
            }
            '=' => {
                if i + 1 < b.len() && b[i + 1] as char == '=' {
                    toks.push(Tok::EqEq);
                    i += 2;
                } else {
                    bail!("unexpected '=' inside expression: {s}");
                }
            }
            '~' => {
                if i + 1 < b.len() && b[i + 1] as char == '=' {
                    toks.push(Tok::NotEq);
                    i += 2;
                } else {
                    bail!("unexpected '~' inside expression: {s}");
                }
            }
            '<' => {
                if i + 1 < b.len() && b[i + 1] as char == '=' {
                    toks.push(Tok::Le);
                    i += 2;
                } else {
                    toks.push(Tok::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < b.len() && b[i + 1] as char == '=' {
                    toks.push(Tok::Ge);
                    i += 2;
                } else {
                    toks.push(Tok::Gt);
                    i += 1;
                }
            }
            '"' => {
                i += 1;
                let start = i;
                let mut out = String::new();
                while i < b.len() {
                    let c = b[i] as char;
                    if c == '\\' {
                        if i + 1 >= b.len() {
                            bail!("unterminated string literal");
                        }
                        let esc = b[i + 1] as char;
                        match esc {
                            'n' => out.push('\n'),
                            'r' => out.push('\r'),
                            't' => out.push('\t'),
                            '\\' => out.push('\\'),
                            '"' => out.push('"'),
                            other => out.push(other),
                        }
                        i += 2;
                        continue;
                    }
                    if c == '"' {
                        break;
                    }
                    out.push(c);
                    i += 1;
                }
                if i >= b.len() || b[i] as char != '"' {
                    bail!("unterminated string literal starting at: {}", &s[start - 1..]);
                }
                i += 1;
                toks.push(Tok::Str(out));
            }
            c if c.is_ascii_digit() => {
                let start = i;
                i += 1;
                while i < b.len() && (b[i] as char).is_ascii_digit() {
                    i += 1;
                }
                let mut is_float = false;
                if i < b.len() && b[i] as char == '.' {
                    is_float = true;
                    i += 1;
                    while i < b.len() && (b[i] as char).is_ascii_digit() {
                        i += 1;
                    }
                }
                let lit = &s[start..i];
                if is_float {
                    toks.push(Tok::Float(lit.parse()?));
                } else {
                    toks.push(Tok::Int(lit.parse()?));
                }
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                i += 1;
                while i < b.len() {
                    let c = b[i] as char;
                    if c.is_ascii_alphanumeric() || c == '_' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let ident = &s[start..i];
                match ident {
                    "nil" => toks.push(Tok::Nil),
                    "true" => toks.push(Tok::True),
                    "false" => toks.push(Tok::False),
                    "and" => toks.push(Tok::And),
                    "or" => toks.push(Tok::Or),
                    _ => toks.push(Tok::Ident(ident.to_string())),
                }
            }
            _ => bail!("unsupported character in expression: {ch}"),
        }
    }
    Ok(toks)
}

#[derive(Clone, Debug)]
enum UnaryOp {
    Neg,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug)]
enum Expr {
    Nil,
    True,
    False,
    Int(i64),
    Float(f32),
    Str(String),
    Var(String),
    Call { name: String, args: Vec<Expr> },
    GlobalTable { idx: u16, key: Box<Expr> },
    LocalTable { idx: i8, key: Box<Expr> },
    Unary { op: UnaryOp, expr: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
}

struct ExprParser {
    toks: Vec<Tok>,
    pos: usize,
}

impl ExprParser {
    fn new(toks: Vec<Tok>) -> Self {
        Self { toks, pos: 0 }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == Some(tok) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse(mut self) -> Result<Expr> {
        let expr = self.parse_or()?;
        if self.pos != self.toks.len() {
            bail!("unexpected trailing tokens in expression");
        }
        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut expr = self.parse_and()?;
        while self.eat(&Tok::Or) {
            let rhs = self.parse_and()?;
            expr = Expr::Binary { op: BinaryOp::Or, left: Box::new(expr), right: Box::new(rhs) };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut expr = self.parse_cmp()?;
        while self.eat(&Tok::And) {
            let rhs = self.parse_cmp()?;
            expr = Expr::Binary { op: BinaryOp::And, left: Box::new(expr), right: Box::new(rhs) };
        }
        Ok(expr)
    }

    fn parse_cmp(&mut self) -> Result<Expr> {
        let mut expr = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Some(Tok::EqEq) => BinaryOp::Eq,
                Some(Tok::NotEq) => BinaryOp::Ne,
                Some(Tok::Lt) => BinaryOp::Lt,
                Some(Tok::Le) => BinaryOp::Le,
                Some(Tok::Gt) => BinaryOp::Gt,
                Some(Tok::Ge) => BinaryOp::Ge,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_add()?;
            expr = Expr::Binary { op, left: Box::new(expr), right: Box::new(rhs) };
        }
        Ok(expr)
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut expr = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinaryOp::Add,
                Some(Tok::Minus) => BinaryOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_mul()?;
            expr = Expr::Binary { op, left: Box::new(expr), right: Box::new(rhs) };
        }
        Ok(expr)
    }

    fn parse_mul(&mut self) -> Result<Expr> {
        let mut expr = self.parse_bitand()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => BinaryOp::Mul,
                Some(Tok::Slash) => BinaryOp::Div,
                Some(Tok::Percent) => BinaryOp::Mod,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_bitand()?;
            expr = Expr::Binary { op, left: Box::new(expr), right: Box::new(rhs) };
        }
        Ok(expr)
    }

    fn parse_bitand(&mut self) -> Result<Expr> {
        let mut expr = self.parse_unary()?;
        while self.eat(&Tok::Amp) {
            let rhs = self.parse_unary()?;
            expr = Expr::Binary { op: BinaryOp::BitAnd, left: Box::new(expr), right: Box::new(rhs) };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if self.eat(&Tok::Minus) {
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary { op: UnaryOp::Neg, expr: Box::new(expr) });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.bump().ok_or_else(|| anyhow!("unexpected end of expression"))? {
            Tok::Nil => Ok(Expr::Nil),
            Tok::True => Ok(Expr::True),
            Tok::False => Ok(Expr::False),
            Tok::Int(v) => Ok(Expr::Int(v)),
            Tok::Float(v) => Ok(Expr::Float(v)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::LParen => {
                let e = self.parse_or()?;
                if !self.eat(&Tok::RParen) {
                    bail!("missing ')' in expression");
                }
                Ok(e)
            }
            Tok::Ident(name) => {
                if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    if !self.eat(&Tok::RParen) {
                        loop {
                            args.push(self.parse_or()?);
                            if self.eat(&Tok::Comma) {
                                continue;
                            }
                            if !self.eat(&Tok::RParen) {
                                bail!("missing ')' after call arguments");
                            }
                            break;
                        }
                    }
                    return Ok(Expr::Call { name, args });
                }

                if (name == "GT" || name == "LT") && self.eat(&Tok::LBracket) {
                    let idx = match self.bump() {
                        Some(Tok::Int(v)) => v,
                        Some(Tok::Minus) => match self.bump() {
                            Some(Tok::Int(v)) => -v,
                            _ => bail!("table index must be integer"),
                        },
                        _ => bail!("table index must be integer"),
                    };
                    if !self.eat(&Tok::RBracket) || !self.eat(&Tok::LBracket) {
                        bail!("table access must be GT[idx][key] or LT[idx][key]");
                    }
                    let key = self.parse_or()?;
                    if !self.eat(&Tok::RBracket) {
                        bail!("table access missing closing ']'");
                    }
                    if name == "GT" {
                        if idx < 0 || idx > i64::from(u16::MAX) {
                            bail!("GT index out of range: {idx}");
                        }
                        return Ok(Expr::GlobalTable { idx: idx as u16, key: Box::new(key) });
                    }
                    if idx < i64::from(i8::MIN) || idx > i64::from(i8::MAX) {
                        bail!("LT index out of range: {idx}");
                    }
                    return Ok(Expr::LocalTable { idx: idx as i8, key: Box::new(key) });
                }

                Ok(Expr::Var(name))
            }
            other => bail!("unexpected token in expression: {:?}", other),
        }
    }
}

fn parse_expr(expr: &str) -> Result<Expr> {
    let toks = tokenize_expr(expr)?;
    ExprParser::new(toks).parse()
}

fn emit_call(name: &str, args: &[Expr], meta: &Meta, user_fns: &HashSet<String>, layout: &GlobalLayout, args_count: i8, out: &mut Vec<Item>) -> Result<()> {
    for arg in args {
        compile_expr(arg, args_count, meta, user_fns, layout, out)?;
    }

    if let Some(sid) = meta.syscall_id_by_name(name) {
        if let Some(expect) = meta.syscall_args_by_id(sid) {
            if usize::from(expect) != args.len() {
                bail!("syscall {name} expects {expect} args, got {}", args.len());
            }
        }
        out.push(Item::Op(OpKind::Syscall { id: sid }));
        return Ok(());
    }

    if name.starts_with("f_") || user_fns.contains(name) {
        out.push(Item::Op(OpKind::CallFn { name: name.to_string() }));
        return Ok(());
    }

    bail!("unknown callee: {name}")
}

fn compile_expr(expr: &Expr, args_count: i8, meta: &Meta, user_fns: &HashSet<String>, layout: &GlobalLayout, out: &mut Vec<Item>) -> Result<()> {
    match expr {
        Expr::Nil => out.push(Item::Op(OpKind::PushNil)),
        Expr::True => out.push(Item::Op(OpKind::PushTrue)),
        Expr::False => bail!("false is not supported as a runtime value in this compiler"),
        Expr::Int(v) => out.push(Item::Op(push_int(*v)?)),
        Expr::Float(v) => out.push(Item::Op(OpKind::PushF32(*v))),
        Expr::Str(s) => out.push(Item::Op(OpKind::PushString(s.clone()))),
        Expr::Var(name) => {
            let re_s = Regex::new(r"^S\d+$").unwrap();
            let re_slot = Regex::new(r"^(a\d+|l\d+)$").unwrap();
            if name == "__ret" {
                out.push(Item::Op(OpKind::PushReturn));
            } else if let Some(idx) = layout.global_idx(name) {
                out.push(Item::Op(OpKind::PushGlobal(idx)));
            } else if re_slot.is_match(name) {
                let idx = slot_to_stack_idx(name, args_count)?;
                out.push(Item::Op(OpKind::PushStack(idx)));
            } else if re_s.is_match(name) {
                out.push(Item::Op(OpKind::PushTop));
            } else {
                bail!("unsupported variable reference: {name}");
            }
        }
        Expr::Call { name, args } => {
            emit_call(name, args, meta, user_fns, layout, args_count, out)?;
            out.push(Item::Op(OpKind::PushReturn));
        }
        Expr::GlobalTable { idx, key } => {
            compile_expr(key, args_count, meta, user_fns, layout, out)?;
            out.push(Item::Op(OpKind::PushGlobalTable(*idx)));
        }
        Expr::LocalTable { idx, key } => {
            compile_expr(key, args_count, meta, user_fns, layout, out)?;
            out.push(Item::Op(OpKind::PushLocalTable(*idx)));
        }
        Expr::Unary { op: UnaryOp::Neg, expr } => {
            compile_expr(expr, args_count, meta, user_fns, layout, out)?;
            out.push(Item::Op(OpKind::Neg));
        }
        Expr::Binary { op, left, right } => {
            if *op == BinaryOp::Ne {
                if let Expr::Binary { op: BinaryOp::BitAnd, left: bleft, right: bright } = &**left {
                    if matches!(&**right, Expr::Int(0)) {
                        compile_expr(bleft, args_count, meta, user_fns, layout, out)?;
                        compile_expr(bright, args_count, meta, user_fns, layout, out)?;
                        out.push(Item::Op(OpKind::BitTest));
                        return Ok(());
                    }
                }
            }
            if *op == BinaryOp::And || *op == BinaryOp::Or {
                compile_expr(left, args_count, meta, user_fns, layout, out)?;
                compile_expr(right, args_count, meta, user_fns, layout, out)?;
                out.push(Item::Op(match op {
                    BinaryOp::And => OpKind::And,
                    BinaryOp::Or => OpKind::Or,
                    _ => unreachable!(),
                }));
                return Ok(());
            }
            if *op == BinaryOp::BitAnd {
                bail!("plain bitwise '&' values are not supported, use '(x & y) ~= 0'");
            }
            compile_expr(left, args_count, meta, user_fns, layout, out)?;
            compile_expr(right, args_count, meta, user_fns, layout, out)?;
            let inst = match op {
                BinaryOp::Add => OpKind::Add,
                BinaryOp::Sub => OpKind::Sub,
                BinaryOp::Mul => OpKind::Mul,
                BinaryOp::Div => OpKind::Div,
                BinaryOp::Mod => OpKind::Mod,
                BinaryOp::Eq => OpKind::SetE,
                BinaryOp::Ne => OpKind::SetNe,
                BinaryOp::Lt => OpKind::SetL,
                BinaryOp::Le => OpKind::SetLe,
                BinaryOp::Gt => OpKind::SetG,
                BinaryOp::Ge => OpKind::SetGe,
                BinaryOp::BitAnd | BinaryOp::And | BinaryOp::Or => unreachable!(),
            };
            out.push(Item::Op(inst));
        }
    }
    Ok(())
}

fn split_assignment(stmt: &str) -> Option<(String, String)> {
    let b = stmt.as_bytes();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut in_string = false;
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i] as char;
        if in_string {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_brack += 1,
            ']' => depth_brack -= 1,
            '=' if depth_paren == 0 && depth_brack == 0 => {
                let prev = if i > 0 { Some(b[i - 1] as char) } else { None };
                let next = if i + 1 < b.len() { Some(b[i + 1] as char) } else { None };
                if prev != Some('=') && prev != Some('~') && next != Some('=') {
                    let lhs = stmt[..i].trim().to_string();
                    let rhs = stmt[i + 1..].trim().to_string();
                    return Some((lhs, rhs));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

enum AssignTarget {
    Stack(i8),
    Global(u16),
    StackTemp,
    GlobalTable(u16, Expr),
    LocalTable(i8, Expr),
}

fn parse_assign_target(lhs: &str, args_count: i8, layout: &GlobalLayout) -> Result<AssignTarget> {
    let re_slot = Regex::new(r"^(a\d+|l\d+)$").unwrap();
    let re_s = Regex::new(r"^S\d+$").unwrap();
    if re_slot.is_match(lhs) {
        return Ok(AssignTarget::Stack(slot_to_stack_idx(lhs, args_count)?));
    }
    if re_s.is_match(lhs) {
        return Ok(AssignTarget::StackTemp);
    }
    if let Some(idx) = layout.global_idx(lhs) {
        return Ok(AssignTarget::Global(idx));
    }

    let re_gt = Regex::new(r"^GT\[(\d+)\]\[(.+)\]$").unwrap();
    if let Some(c) = re_gt.captures(lhs) {
        let idx: u16 = c.get(1).unwrap().as_str().parse()?;
        let key = parse_expr(c.get(2).unwrap().as_str().trim())?;
        return Ok(AssignTarget::GlobalTable(idx, key));
    }
    let re_lt = Regex::new(r"^LT\[(-?\d+)\]\[(.+)\]$").unwrap();
    if let Some(c) = re_lt.captures(lhs) {
        let idx: i8 = c.get(1).unwrap().as_str().parse()?;
        let key = parse_expr(c.get(2).unwrap().as_str().trim())?;
        return Ok(AssignTarget::LocalTable(idx, key));
    }

    bail!("unsupported assignment target: {lhs}")
}

fn compile_simple_stmt(
    stmt: &str,
    args_count: i8,
    meta: &Meta,
    user_fns: &HashSet<String>,
    layout: &GlobalLayout,
    out: &mut Vec<Item>,
) -> Result<()> {
    let s = stmt.trim();
    if s.is_empty() {
        return Ok(());
    }

    let ignore_re = Regex::new(r#"^__ret\s*=\s*(nil|true|false|-?\d+(?:\.\d+)?|\"(?:\\.|[^\"])*\")\s*$"#).unwrap();
    if ignore_re.is_match(s) {
        return Ok(());
    }

    if let Some((lhs, rhs)) = split_assignment(s) {
        if lhs == "__ret" {
            let expr = parse_expr(&rhs)?;
            if let Expr::Call { name, args } = expr {
                emit_call(&name, &args, meta, user_fns, layout, args_count, out)?;
                return Ok(());
            }
            bail!("__ret assignment requires a call: {s}");
        }

        let target = parse_assign_target(&lhs, args_count, layout)?;
        let expr = parse_expr(&rhs)?;
        match target {
            AssignTarget::Stack(idx) => {
                compile_expr(&expr, args_count, meta, user_fns, layout, out)?;
                out.push(Item::Op(OpKind::PopStack(idx)));
            }
            AssignTarget::Global(idx) => {
                compile_expr(&expr, args_count, meta, user_fns, layout, out)?;
                out.push(Item::Op(OpKind::PopGlobal(idx)));
            }
            AssignTarget::StackTemp => {
                compile_expr(&expr, args_count, meta, user_fns, layout, out)?;
            }
            AssignTarget::GlobalTable(idx, key) => {
                compile_expr(&key, args_count, meta, user_fns, layout, out)?;
                compile_expr(&expr, args_count, meta, user_fns, layout, out)?;
                out.push(Item::Op(OpKind::PopGlobalTable(idx)));
            }
            AssignTarget::LocalTable(idx, key) => {
                compile_expr(&key, args_count, meta, user_fns, layout, out)?;
                compile_expr(&expr, args_count, meta, user_fns, layout, out)?;
                out.push(Item::Op(OpKind::PopLocalTable(idx)));
            }
        }
        return Ok(());
    }

    let expr = parse_expr(s)?;
    if let Expr::Call { name, args } = expr {
        emit_call(&name, &args, meta, user_fns, layout, args_count, out)?;
        return Ok(());
    }

    bail!("unsupported statement: {s}")
}

struct LabelGen {
    prefix: String,
    n: u32,
}

impl LabelGen {
    fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            n: 0,
        }
    }

    fn fresh(&mut self, kind: &str) -> String {
        let id = self.n;
        self.n += 1;
        format!("{}:{}:{}", self.prefix, kind, id)
    }
}

fn compile_cond_generic(
    cond: &str,
    args_count: i8,
    meta: &Meta,
    user_fns: &HashSet<String>,
    layout: &GlobalLayout,
    out: &mut Vec<Item>,
) -> Result<()> {
    let expr = parse_expr(cond)?;
    compile_expr(&expr, args_count, meta, user_fns, layout, out)
}

fn compile_stmts(
    stmts: &[Stmt],
    args_count: i8,
    meta: &Meta,
    user_fns: &HashSet<String>,
    layout: &GlobalLayout,
    out: &mut Vec<Item>,
    lg: &mut LabelGen,
    break_stack: &mut Vec<String>,
) -> Result<()> {
    for st in stmts {
        match st {
            Stmt::Simple(s) => compile_simple_stmt(s, args_count, meta, user_fns, layout, out)?,
            Stmt::Return(None) => out.push(Item::Op(OpKind::Ret)),
            Stmt::Return(Some(expr)) => {
                let e = parse_expr(expr)?;
                compile_expr(&e, args_count, meta, user_fns, layout, out)?;
                out.push(Item::Op(OpKind::Retv));
            }
            Stmt::Break => {
                let tgt = break_stack
                    .last()
                    .ok_or_else(|| anyhow!("break outside of loop"))?
                    .clone();
                out.push(Item::Op(OpKind::JmpLabel { label: tgt }));
            }
            Stmt::If { arms, else_arm } => {
                let end_lbl = lg.fresh("if_end");
                for (idx, (cond, body)) in arms.iter().enumerate() {
                    let after_lbl = lg.fresh(&format!("if_next_{idx}"));
                    match parse_cond(cond) {
                        CondKind::AlwaysTrue => {
                            compile_stmts(body, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                            out.push(Item::Op(OpKind::JmpLabel { label: end_lbl.clone() }));
                            break;
                        }
                        CondKind::AlwaysFalse => {
                            out.push(Item::Label(Label::new(after_lbl.clone())));
                        }
                        CondKind::NonZero => {
                            out.push(Item::Op(OpKind::JzLabel { label: after_lbl.clone() }));
                            compile_stmts(body, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                            out.push(Item::Op(OpKind::JmpLabel { label: end_lbl.clone() }));
                            out.push(Item::Label(Label::new(after_lbl)));
                        }
                        CondKind::Zero => {
                            let body_lbl = lg.fresh(&format!("if_body_{idx}"));
                            out.push(Item::Op(OpKind::JzLabel { label: body_lbl.clone() }));
                            out.push(Item::Op(OpKind::JmpLabel { label: after_lbl.clone() }));
                            out.push(Item::Label(Label::new(body_lbl)));
                            compile_stmts(body, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                            out.push(Item::Op(OpKind::JmpLabel { label: end_lbl.clone() }));
                            out.push(Item::Label(Label::new(after_lbl)));
                        }
                        CondKind::Generic => {
                            compile_cond_generic(cond, args_count, meta, user_fns, layout, out)?;
                            out.push(Item::Op(OpKind::JzLabel { label: after_lbl.clone() }));
                            compile_stmts(body, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                            out.push(Item::Op(OpKind::JmpLabel { label: end_lbl.clone() }));
                            out.push(Item::Label(Label::new(after_lbl)));
                        }
                    }
                }
                if let Some(eb) = else_arm {
                    compile_stmts(eb, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                }
                out.push(Item::Label(Label::new(end_lbl)));
            }
            Stmt::While { cond, body } => {
                let head = lg.fresh("while_head");
                let end = lg.fresh("while_end");
                let body_lbl = lg.fresh("while_body");

                out.push(Item::Label(Label::new(head.clone())));
                break_stack.push(end.clone());

                match parse_cond(cond) {
                    CondKind::AlwaysTrue => {
                        compile_stmts(body, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                        out.push(Item::Op(OpKind::JmpLabel { label: head }));
                    }
                    CondKind::AlwaysFalse => {
                        out.push(Item::Op(OpKind::JmpLabel { label: end.clone() }));
                    }
                    CondKind::NonZero => {
                        out.push(Item::Op(OpKind::JzLabel { label: end.clone() }));
                        compile_stmts(body, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                        out.push(Item::Op(OpKind::JmpLabel { label: head }));
                    }
                    CondKind::Zero => {
                        out.push(Item::Op(OpKind::JzLabel { label: body_lbl.clone() }));
                        out.push(Item::Op(OpKind::JmpLabel { label: end.clone() }));
                        out.push(Item::Label(Label::new(body_lbl)));
                        compile_stmts(body, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                        out.push(Item::Op(OpKind::JmpLabel { label: head }));
                    }
                    CondKind::Generic => {
                        compile_cond_generic(cond, args_count, meta, user_fns, layout, out)?;
                        out.push(Item::Op(OpKind::JzLabel { label: end.clone() }));
                        compile_stmts(body, args_count, meta, user_fns, layout, out, lg, break_stack)?;
                        out.push(Item::Op(OpKind::JmpLabel { label: head }));
                    }
                }

                break_stack.pop();
                out.push(Item::Label(Label::new(end)));
            }
        }
    }
    Ok(())
}

// ----------------------------
// __pc dispatcher mode
// ----------------------------

fn looks_like_pc_dispatcher(raw: &[String]) -> bool {
    let mut saw_pc = false;
    let mut saw_case = false;
    let mut saw_while_true = false;
    for ln in raw {
        let t = ln.trim();
        if t.contains("__pc") {
            saw_pc = true;
        }
        if t == "while true do" {
            saw_while_true = true;
        }
        if t.starts_with("if __pc ==") || t.starts_with("elseif __pc ==") {
            saw_case = true;
        }
    }
    saw_pc && saw_while_true && saw_case
}

fn is_comment_or_empty_line(t: &str) -> bool {
    let tt = t.trim();
    tt.is_empty() || tt.starts_with("--")
}

fn is_if_start_line(t: &str) -> bool {
    let tt = t.trim();
    tt.starts_with("if ") && tt.ends_with(" then")
}

fn is_while_start_line(t: &str) -> bool {
    let tt = t.trim();
    tt.starts_with("while ") && tt.ends_with(" do")
}

fn is_for_start_line(t: &str) -> bool {
    let tt = t.trim();
    tt.starts_with("for ") && tt.ends_with(" do")
}

fn is_repeat_start_line(t: &str) -> bool {
    t.trim() == "repeat"
}

fn is_until_line(t: &str) -> bool {
    t.trim().starts_with("until ")
}

fn is_end_line(t: &str) -> bool {
    t.trim() == "end"
}

fn bb_label(fn_name: &str, pc: u32) -> String {
    format!("bb:{fn_name}:{pc}")
}

fn parse_entry_pc(body: &[String]) -> u32 {
    let re = Regex::new(r"^(?:local\s+)?__pc\s*=\s*(\d+)\s*$").unwrap();
    for ln in body {
        let t = ln.trim();
        if let Some(c) = re.captures(t) {
            if let Ok(v) = c.get(1).unwrap().as_str().parse::<u32>() {
                return v;
            }
        }
        if t == "while true do" {
            break;
        }
    }
    0
}

fn collect_case_body(body: &[String], mut i: usize, re_case: &Regex) -> (Vec<String>, usize) {
    let mut out: Vec<String> = Vec::new();
    let mut nest: i32 = 0;

    while i < body.len() {
        let t = body[i].trim();
        if nest == 0 {
            if re_case.is_match(t) || t == "else" {
                break;
            }
        }

        out.push(body[i].clone());

        if is_if_start_line(t) || is_while_start_line(t) || is_for_start_line(t) || is_repeat_start_line(t) {
            nest += 1;
        } else if is_end_line(t) {
            nest -= 1;
        } else if is_until_line(t) {
            nest -= 1;
        }

        i += 1;
    }

    (out, i)
}

fn compile_pc_case(
    pc: u32,
    lines: &[String],
    fn_name: &str,
    args_count: i8,
    meta: &Meta,
    user_fns: &HashSet<String>,
    layout: &GlobalLayout,
    out: &mut Vec<Item>,
) -> Result<()> {
    out.push(Item::Label(Label::new(bb_label(fn_name, pc))));

    let re_pc_set = Regex::new(r"^__pc\s*=\s*(\d+)\s*$").unwrap();
    let re_term_if = Regex::new(r"^if\s+S\d+\s*(==|~=)\s*0\s+then\s*$").unwrap();

    let mut i = 0usize;
    while i < lines.len() {
        let mut t = lines[i].trim().to_string();
        if is_comment_or_empty_line(&t) {
            i += 1;
            continue;
        }

        if t.starts_with("local ") && !t.contains('=') {
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("local ") {
            t = rest.trim().to_string();
        }

        if t == "return" {
            out.push(Item::Op(OpKind::Ret));
            return Ok(());
        }

        if let Some(rest) = t.strip_prefix("return ") {
            let e = parse_expr(rest.trim())?;
            compile_expr(&e, args_count, meta, user_fns, layout, out)?;
            out.push(Item::Op(OpKind::Retv));
            return Ok(());
        }

        if let Some(c) = re_pc_set.captures(&t) {
            let target: u32 = c.get(1).unwrap().as_str().parse()?;
            out.push(Item::Op(OpKind::JmpLabel {
                label: bb_label(fn_name, target),
            }));
            return Ok(());
        }

        if let Some(c) = re_term_if.captures(&t) {
            let op = c.get(1).unwrap().as_str();
            let mut j = i + 1;
            while j < lines.len() && is_comment_or_empty_line(lines[j].trim()) {
                j += 1;
            }
            if j >= lines.len() {
                bail!("unterminated pc-if in bb {pc}");
            }
            let then_line = lines[j].trim();
            let then_pc: u32 = re_pc_set
                .captures(then_line)
                .ok_or_else(|| anyhow!("pc-if then arm must set __pc in bb {pc}"))?
                .get(1)
                .unwrap()
                .as_str()
                .parse()?;

            j += 1;
            while j < lines.len() && is_comment_or_empty_line(lines[j].trim()) {
                j += 1;
            }
            if j >= lines.len() || lines[j].trim() != "else" {
                bail!("pc-if missing else in bb {pc}");
            }

            j += 1;
            while j < lines.len() && is_comment_or_empty_line(lines[j].trim()) {
                j += 1;
            }
            if j >= lines.len() {
                bail!("pc-if missing else pc assignment in bb {pc}");
            }
            let else_line = lines[j].trim();
            let else_pc: u32 = re_pc_set
                .captures(else_line)
                .ok_or_else(|| anyhow!("pc-if else arm must set __pc in bb {pc}"))?
                .get(1)
                .unwrap()
                .as_str()
                .parse()?;

            j += 1;
            while j < lines.len() && is_comment_or_empty_line(lines[j].trim()) {
                j += 1;
            }
            if j >= lines.len() || lines[j].trim() != "end" {
                bail!("pc-if missing end in bb {pc}");
            }

            let (zero_target, nonzero_target) = if op == "==" {
                (then_pc, else_pc)
            } else {
                (else_pc, then_pc)
            };
            out.push(Item::Op(OpKind::JzLabel {
                label: bb_label(fn_name, zero_target),
            }));
            out.push(Item::Op(OpKind::JmpLabel {
                label: bb_label(fn_name, nonzero_target),
            }));
            return Ok(());
        }

        compile_simple_stmt(&t, args_count, meta, user_fns, layout, out)?;
        i += 1;
    }

    out.push(Item::Op(OpKind::Ret));
    Ok(())
}

fn compile_pc_dispatcher_function(
    f: &Function,
    meta: &Meta,
    user_fns: &HashSet<String>,
    layout: &GlobalLayout,
    out: &mut Vec<Item>,
) -> Result<()> {
    if f.raw.len() < 2 {
        bail!("function {}: too short", f.name);
    }
    let body: Vec<String> = f.raw[1..f.raw.len() - 1].to_vec();

    let entry_pc = parse_entry_pc(&body);

    let re_case = Regex::new(r"^(if|elseif)\s+__pc\s*==\s*(\d+)\s+then\s*$").unwrap();

    let mut i = 0usize;
    while i < body.len() {
        if re_case.is_match(body[i].trim()) {
            break;
        }
        i += 1;
    }
    if i >= body.len() {
        bail!("function {}: pc-dispatcher header not found", f.name);
    }

    let mut cases: Vec<(u32, Vec<String>)> = Vec::new();
    while i < body.len() {
        let t = body[i].trim();
        if t == "else" {
            break;
        }
        if let Some(c) = re_case.captures(t) {
            let pc: u32 = c.get(2).unwrap().as_str().parse()?;
            i += 1;
            let (case_lines, next_i) = collect_case_body(&body, i, &re_case);
            cases.push((pc, case_lines));
            i = next_i;
            continue;
        }
        i += 1;
    }

    if cases.is_empty() {
        bail!("function {}: no pc-dispatcher cases found", f.name);
    }

    if let Some(pos) = cases.iter().position(|(pc, _)| *pc == entry_pc) {
        if pos != 0 {
            let entry = cases.remove(pos);
            cases.insert(0, entry);
        }
    }

    for (pc, lines) in cases {
        compile_pc_case(pc, &lines, &f.name, f.args_count, meta, user_fns, layout, out)?;
    }

    Ok(())
}

pub fn compile_program(meta: &Meta, program: &Program) -> Result<(Vec<Item>, GlobalLayout)> {
    let mut items: Vec<Item> = Vec::new();
    let layout = GlobalLayout::from_globals(&program.globals)?;
    let user_fns: HashSet<String> = program.functions.iter().map(|f| f.name.clone()).collect();

    for f in &program.functions {
        items.push(Item::Label(Label::new(format!("fn:{}", f.name))));
        items.push(Item::Op(OpKind::InitStack {
            args: f.args_count,
            locals: f.locals_count,
        }));

        if looks_like_pc_dispatcher(&f.raw) {
            compile_pc_dispatcher_function(f, meta, &user_fns, &layout, &mut items)?;
        } else {
            let mut lg = LabelGen::new(format!("fn:{}", f.name));
            let mut break_stack: Vec<String> = Vec::new();
            compile_stmts(
                &f.body,
                f.args_count,
                meta,
                &user_fns,
                &layout,
                &mut items,
                &mut lg,
                &mut break_stack,
            )?;
        }

        if !matches!(items.last(), Some(Item::Op(OpKind::Ret | OpKind::Retv))) {
            items.push(Item::Op(OpKind::Ret));
        }
    }

    Ok((items, layout))
}
