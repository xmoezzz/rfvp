use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub args_count: i8,
    pub locals_count: i8,
    pub body: Vec<Stmt>,
    // Raw lines for scanning/inference
    pub raw: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlobalKind {
    NonVolatile,
    Volatile,
}

#[derive(Clone, Debug)]
pub struct GlobalDecl {
    pub name: String,
    pub kind: GlobalKind,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub globals: Vec<GlobalDecl>,
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Simple(String),
    Return(Option<String>),
    Break,
    If {
        arms: Vec<(String, Vec<Stmt>)>,
        else_arm: Option<Vec<Stmt>>,
    },
    While {
        cond: String,
        body: Vec<Stmt>,
    },
}

fn is_comment_or_empty(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.starts_with("--")
}

fn strip_local(stmt: &str) -> &str {
    let t = stmt.trim_start();
    if let Some(rest) = t.strip_prefix("local ") {
        return rest.trim_start();
    }
    t
}

fn is_if_start(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("if ") && t.ends_with(" then")
}

fn is_elseif(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("elseif ") && t.ends_with(" then")
}

fn is_else(s: &str) -> bool {
    s.trim() == "else"
}

fn is_while_start(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("while ") && t.ends_with(" do")
}

fn is_end(s: &str) -> bool {
    s.trim() == "end"
}

fn extract_if_cond(line: &str) -> Result<String> {
    let t = line.trim();
    if !t.ends_with(" then") {
        bail!("invalid if header: {t}");
    }
    let inner = t
        .strip_prefix("if ")
        .or_else(|| t.strip_prefix("elseif "))
        .ok_or_else(|| anyhow!("invalid if header: {t}"))?;
    let inner = inner.trim_end_matches(" then");
    Ok(inner.trim().to_string())
}

fn extract_while_cond(line: &str) -> Result<String> {
    let t = line.trim();
    if !t.ends_with(" do") {
        bail!("invalid while header: {t}");
    }
    let inner = t
        .strip_prefix("while ")
        .ok_or_else(|| anyhow!("invalid while header: {t}"))?;
    let inner = inner.trim_end_matches(" do");
    Ok(inner.trim().to_string())
}

fn parse_block(lines: &[String], i: &mut usize, stop_on: &[&str]) -> Result<Vec<Stmt>> {
    let mut out: Vec<Stmt> = Vec::new();

    while *i < lines.len() {
        let line0 = lines[*i].clone();
        let line = line0.trim();

        if is_comment_or_empty(line) {
            *i += 1;
            continue;
        }

        if stop_on.iter().any(|tok| match *tok {
            "elseif" => is_elseif(line),
            "else" => is_else(line),
            "end" => is_end(line),
            _ => false,
        }) {
            break;
        }

        if is_if_start(line) {
            let cond = extract_if_cond(line)?;
            *i += 1;
            let then_block = parse_block(lines, i, &["elseif", "else", "end"])?;

            let mut arms: Vec<(String, Vec<Stmt>)> = vec![(cond, then_block)];
            while *i < lines.len() && is_elseif(lines[*i].trim()) {
                let c = extract_if_cond(lines[*i].trim())?;
                *i += 1;
                let b = parse_block(lines, i, &["elseif", "else", "end"])?;
                arms.push((c, b));
            }

            let mut else_arm: Option<Vec<Stmt>> = None;
            if *i < lines.len() && is_else(lines[*i].trim()) {
                *i += 1;
                let b = parse_block(lines, i, &["end"])?;
                else_arm = Some(b);
            }

            if *i >= lines.len() || !is_end(lines[*i].trim()) {
                bail!("if without closing end");
            }
            *i += 1;

            out.push(Stmt::If { arms, else_arm });
            continue;
        }

        if is_while_start(line) {
            let cond = extract_while_cond(line)?;
            *i += 1;
            let body = parse_block(lines, i, &["end"])?;
            if *i >= lines.len() || !is_end(lines[*i].trim()) {
                bail!("while without closing end");
            }
            *i += 1;
            out.push(Stmt::While { cond, body });
            continue;
        }

        if line == "break" {
            *i += 1;
            out.push(Stmt::Break);
            continue;
        }

        if line == "return" {
            *i += 1;
            out.push(Stmt::Return(None));
            continue;
        }

        if let Some(rest) = line.strip_prefix("return ") {
            *i += 1;
            out.push(Stmt::Return(Some(rest.trim().to_string())));
            continue;
        }

        // Local declarations are not semantic. Keep assignments.
        if line.starts_with("local ") && !line.contains('=') {
            *i += 1;
            continue;
        }

        let simple = strip_local(line).to_string();
        *i += 1;
        out.push(Stmt::Simple(simple));
    }

    Ok(out)
}

fn split_functions(lines: &[String], start_idx: usize) -> Result<Vec<Vec<String>>> {
    let head_re = Regex::new(r"^(?:local\s+)?function\s+").unwrap();

    let mut out: Vec<Vec<String>> = Vec::new();
    let mut i = start_idx;
    while i < lines.len() {
        if head_re.is_match(lines[i].trim()) {
            let start = i;
            let mut nest = 1i32;
            i += 1;
            while i < lines.len() && nest > 0 {
                let t = lines[i].trim();
                if head_re.is_match(t) {
                    nest += 1;
                } else if is_if_start(t) {
                    nest += 1;
                } else if is_while_start(t) {
                    nest += 1;
                } else if is_end(t) {
                    nest -= 1;
                }
                i += 1;
            }
            out.push(lines[start..i].to_vec());
        } else if is_comment_or_empty(lines[i].trim()) {
            i += 1;
        } else {
            bail!("unsupported top-level statement: {}", lines[i].trim());
        }
    }

    if out.is_empty() {
        bail!("no functions found in Lua");
    }

    Ok(out)
}

fn parse_global_line(line: &str, seen: &mut HashSet<String>, out: &mut Vec<GlobalDecl>) -> Result<()> {
    let t = line.trim();
    let (kind, rest) = if let Some(rest) = t.strip_prefix("global ") {
        (GlobalKind::NonVolatile, rest.trim())
    } else if let Some(rest) = t.strip_prefix("volatile global ") {
        (GlobalKind::Volatile, rest.trim())
    } else {
        bail!("unsupported top-level statement: {t}");
    };

    if rest.is_empty() {
        bail!("empty global declaration: {t}");
    }
    if rest.contains('=') {
        bail!("global initializers are not supported: {t}");
    }

    let re_g = Regex::new(r"^g\d+$").unwrap();
    let re_vg = Regex::new(r"^vg\d+$").unwrap();

    for raw_name in rest.split(',') {
        let name = raw_name.trim();
        if name.is_empty() {
            bail!("empty global name in declaration: {t}");
        }
        match kind {
            GlobalKind::NonVolatile => {
                if !re_g.is_match(name) {
                    bail!("non-volatile globals must be named gN: {name}");
                }
            }
            GlobalKind::Volatile => {
                if !re_vg.is_match(name) {
                    bail!("volatile globals must be named vgN: {name}");
                }
            }
        }
        if !seen.insert(name.to_string()) {
            bail!("duplicate global declaration: {name}");
        }
        out.push(GlobalDecl {
            name: name.to_string(),
            kind: kind.clone(),
        });
    }

    Ok(())
}

pub fn parse_lua(path: &Path) -> Result<Program> {
    let txt = std::fs::read_to_string(path).with_context(|| format!("read lua: {}", path.display()))?;
    let lines: Vec<String> = txt.lines().map(|s| s.to_string()).collect();
    let head_re = Regex::new(r"^(?:local\s+)?function\s+").unwrap();

    let mut globals: Vec<GlobalDecl> = Vec::new();
    let mut seen_globals: HashSet<String> = HashSet::new();
    let mut first_fn_idx = None;

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if is_comment_or_empty(t) {
            continue;
        }
        if head_re.is_match(t) {
            first_fn_idx = Some(i);
            break;
        }
        parse_global_line(t, &mut seen_globals, &mut globals)?;
    }

    let start_idx = first_fn_idx.ok_or_else(|| anyhow!("no functions found in Lua"))?;
    let funcs_lines = split_functions(&lines, start_idx)?;

    let head_re = Regex::new(r"^(?:local\s+)?function\s+([A-Za-z_]\w*)\s*\(([^)]*)\)\s*$").unwrap();
    let re_a = Regex::new(r"\ba(\d+)\b").unwrap();
    let re_l = Regex::new(r"\bl(\d+)\b").unwrap();

    let mut funs: Vec<Function> = Vec::new();
    for fl in funcs_lines {
        if fl.is_empty() {
            continue;
        }
        let head = fl[0].trim();
        let caps = head_re
            .captures(head)
            .ok_or_else(|| anyhow!("unexpected function header: {head}"))?;
        let name = caps.get(1).unwrap().as_str().to_string();
        let args_s = caps.get(2).unwrap().as_str().trim();
        let args: Vec<&str> = if args_s.is_empty() {
            vec![]
        } else {
            args_s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).collect()
        };

        let mut max_a: Option<u32> = None;
        for a in &args {
            if let Some(mm) = re_a.captures(a) {
                let v: u32 = mm.get(1).unwrap().as_str().parse().unwrap_or(0);
                max_a = Some(max_a.map(|x| x.max(v)).unwrap_or(v));
            }
        }
        for ln in &fl {
            for mm in re_a.captures_iter(ln) {
                let v: u32 = mm.get(1).unwrap().as_str().parse().unwrap_or(0);
                max_a = Some(max_a.map(|x| x.max(v)).unwrap_or(v));
            }
        }
        let args_count = i8::try_from(max_a.map(|x| x + 1).unwrap_or(0))
            .map_err(|_| anyhow!("args_count does not fit i8"))?;

        let mut max_l: Option<u32> = None;
        for ln in &fl {
            for mm in re_l.captures_iter(ln) {
                let v: u32 = mm.get(1).unwrap().as_str().parse().unwrap_or(0);
                max_l = Some(max_l.map(|x| x.max(v)).unwrap_or(v));
            }
        }
        let locals_count = i8::try_from(max_l.map(|x| x + 1).unwrap_or(0))
            .map_err(|_| anyhow!("locals_count does not fit i8"))?;

        if fl.len() < 2 {
            bail!("function {name}: too short");
        }
        let body_lines: Vec<String> = fl[1..fl.len() - 1].to_vec();
        let mut idx = 0usize;
        let body = parse_block(&body_lines, &mut idx, &[])?;

        funs.push(Function {
            name,
            args_count,
            locals_count,
            body,
            raw: fl,
        });
    }

    Ok(Program {
        globals,
        functions: funs,
    })
}
