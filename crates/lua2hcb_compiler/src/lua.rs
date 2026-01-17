use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
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

        // Local declarations emitted by the decompiler are not semantic (locals are inferred).
        // Examples:
        //   local S0, S1, S2
        //   local __ret = nil
        // We skip pure declarations and keep assignments.
        if line.starts_with("local ") && !line.contains('=') {
            *i += 1;
            continue;
        }

        // Treat everything else as a simple IR line.
        let simple = strip_local(line).to_string();
        *i += 1;
        out.push(Stmt::Simple(simple));
    }

    Ok(out)
}

fn split_functions(lua_text: &str) -> Result<Vec<Vec<String>>> {
    let lines: Vec<String> = lua_text.lines().map(|s| s.to_string()).collect();
    let head_re = Regex::new(r"^(?:local\s+)?function\s+").unwrap();

    let mut out: Vec<Vec<String>> = Vec::new();
    let mut i = 0usize;
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
                    // IMPORTANT: `elseif` does not introduce a new block that needs its own `end`.
                    // It is part of the same `if ... then ... elseif ... then ... end` construct.
                    // Counting it as a new nested block breaks function splitting and can cause
                    // later function definitions to be swallowed into the current function.
                    nest += 1;
                } else if is_while_start(t) {
                    nest += 1;
                } else if is_end(t) {
                    nest -= 1;
                }
                i += 1;
            }
            out.push(lines[start..i].to_vec());
        } else {
            i += 1;
        }
    }

    if out.is_empty() {
        bail!("no functions found in Lua");
    }

    Ok(out)
}

pub fn parse_lua(path: &Path) -> Result<Vec<Function>> {
    let txt = std::fs::read_to_string(path).with_context(|| format!("read lua: {}", path.display()))?;
    let funcs_lines = split_functions(&txt)?;

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

        // args_count: max aN seen in header or body + 1.
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

        // locals_count: max lN seen in body + 1.
        let mut max_l: Option<u32> = None;
        for ln in &fl {
            for mm in re_l.captures_iter(ln) {
                let v: u32 = mm.get(1).unwrap().as_str().parse().unwrap_or(0);
                max_l = Some(max_l.map(|x| x.max(v)).unwrap_or(v));
            }
        }
        let locals_count = i8::try_from(max_l.map(|x| x + 1).unwrap_or(0))
            .map_err(|_| anyhow!("locals_count does not fit i8"))?;

        // Body lines: between header and the final end.
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

    Ok(funs)
}
