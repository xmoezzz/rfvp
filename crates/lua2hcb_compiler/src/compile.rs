use crate::ir::{Item, Label, OpKind};
use crate::lua::{Function, Stmt};
use crate::meta::Meta;
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CondKind {
    NonZero,
    Zero,
    AlwaysTrue,
    AlwaysFalse,
}

fn parse_cond(cond: &str) -> CondKind {
    let c = cond.trim();
    if c == "true" {
        return CondKind::AlwaysTrue;
    }
    if c == "false" || c == "nil" {
        return CondKind::AlwaysFalse;
    }

    // Accept optional parentheses.
    let c = c.trim_start_matches('(').trim_end_matches(')').trim();

    // Typical IR conditions.
    // - Sx ~= 0  => NonZero
    // - Sx == 0  => Zero
    // - Sx       => NonZero
    let re_ne0 = Regex::new(r"^S\d+\s*~=\s*0$" ).unwrap();
    let re_eq0 = Regex::new(r"^S\d+\s*==\s*0$" ).unwrap();
    let re_s = Regex::new(r"^S\d+$" ).unwrap();

    if re_ne0.is_match(c) {
        return CondKind::NonZero;
    }
    if re_eq0.is_match(c) {
        return CondKind::Zero;
    }
    if re_s.is_match(c) {
        return CondKind::NonZero;
    }

    // Fallback: treat as non-zero; this keeps the compiler permissive
    // for decompiler-emitted conditions like "(S0 ~= 0)".
    CondKind::NonZero
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
    // Limited support: \\ and \" and \n \r \t.
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

fn compile_simple_stmt(
    stmt: &str,
    args_count: i8,
    meta: &Meta,
    user_fns: &HashSet<String>,
    out: &mut Vec<Item>,
) -> Result<()> {
    let s = stmt.trim();

    if s.is_empty() {
        return Ok(());
    }

    // Some decompiler variants include explicit literal writes to __ret (e.g. `__ret = nil`).
    // The HCB VM's return register is implicitly produced by call/syscall, so these statements
    // are non-semantic and should be ignored.
    {let re = Regex::new(r#"^__ret\s*=\s*(nil|true|false|-?\d+(?:\.\d+)?|"(?:\\.|[^"])*")\s*$"#).unwrap();
        
        if re.is_match(s) {
            return Ok(());
        }
    }

    // __ret = foo(...)
    {
        let re = Regex::new(r"^__ret\s*=\s*(\w+)\s*\((.*)\)\s*$").unwrap();
        if let Some(c) = re.captures(s) {
            let name = c.get(1).unwrap().as_str();
            let args_s = c.get(2).unwrap().as_str().trim();
            let argc = if args_s.is_empty() {
                0usize
            } else {
                args_s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).count()
            };

            if let Some(sid) = meta.syscall_id_by_name(name) {
                out.push(Item::Op(OpKind::Syscall { id: sid }));
                let _ = argc; // argc is for human readability; opcode has only id.
                return Ok(());
            }

            if name.starts_with("f_0x") || user_fns.contains(name) {
                out.push(Item::Op(OpKind::CallFn {
                    name: name.to_string(),
                }));
                let _ = argc;
                return Ok(());
            }

            bail!("unknown callee: {name}");
        }
    }

    // foo(...) as statement
    {
        let re = Regex::new(r"^(\w+)\s*\((.*)\)\s*$").unwrap();
        if let Some(c) = re.captures(s) {
            let name = c.get(1).unwrap().as_str();
            let args_s = c.get(2).unwrap().as_str().trim();
            let _argc = if args_s.is_empty() {
                0usize
            } else {
                args_s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).count()
            };

            if let Some(sid) = meta.syscall_id_by_name(name) {
                out.push(Item::Op(OpKind::Syscall { id: sid }));
                return Ok(());
            }
            if name.starts_with("f_0x") || user_fns.contains(name) {
                out.push(Item::Op(OpKind::CallFn {
                    name: name.to_string(),
                }));
                return Ok(());
            }
        }
    }

    // Sx = ...
    {
        let re = Regex::new(r"^S\d+\s*=\s*(.+)$").unwrap();
        if let Some(c) = re.captures(s) {
            let rhs = c.get(1).unwrap().as_str().trim();

            // Literals
            if rhs == "nil" {
                out.push(Item::Op(OpKind::PushNil));
                return Ok(());
            }
            if rhs == "true" {
                out.push(Item::Op(OpKind::PushTrue));
                return Ok(());
            }
            if rhs == "__ret" {
                out.push(Item::Op(OpKind::PushReturn));
                return Ok(());
            }

            if let Ok(v) = rhs.parse::<i64>() {
                out.push(Item::Op(push_int(v)?));
                return Ok(());
            }

            if let Ok(v) = rhs.parse::<f32>() {
                // Distinguish integer vs float by presence of '.' or exponent.
                if rhs.contains('.') || rhs.contains('e') || rhs.contains('E') {
                    out.push(Item::Op(OpKind::PushF32(v)));
                    return Ok(());
                }
            }

            {
                let re = Regex::new(r#"^"(.*)"$"# ).unwrap();
                if let Some(mm) = re.captures(rhs) {
                    let lit = mm.get(1).unwrap().as_str();
                    out.push(Item::Op(OpKind::PushString(lua_unescape_string(lit))));
                    return Ok(());
                }
            }

            // push_top pattern: Sx = Sy
            {
                let re = Regex::new(r"^S\d+$").unwrap();
                if re.is_match(rhs) {
                    out.push(Item::Op(OpKind::PushTop));
                    return Ok(());
                }
            }

            // push_global
            {
                let re = Regex::new(r"^G\[(\d+)\]$" ).unwrap();
                if let Some(mm) = re.captures(rhs) {
                    let idx: u16 = mm.get(1).unwrap().as_str().parse()?;
                    out.push(Item::Op(OpKind::PushGlobal(idx)));
                    return Ok(());
                }
            }

            // push_stack (frame slots)
            {
                let re = Regex::new(r"^(a\d+|l\d+)$").unwrap();
                if re.is_match(rhs) {
                    let idx = slot_to_stack_idx(rhs, args_count)?;
                    out.push(Item::Op(OpKind::PushStack(idx)));
                    return Ok(());
                }
            }

            // table reads
            {
                let re = Regex::new(r"^GT\[(\d+)\]\[S\d+\]$" ).unwrap();
                if let Some(mm) = re.captures(rhs) {
                    let idx: u16 = mm.get(1).unwrap().as_str().parse()?;
                    out.push(Item::Op(OpKind::PushGlobalTable(idx)));
                    return Ok(());
                }
            }
            {
                let re = Regex::new(r"^LT\[(-?\d+)\]\[S\d+\]$" ).unwrap();
                if let Some(mm) = re.captures(rhs) {
                    let idx: i8 = mm.get(1).unwrap().as_str().parse()?;
                    out.push(Item::Op(OpKind::PushLocalTable(idx)));
                    return Ok(());
                }
            }

            // unary neg: -Sx
            {
                let re = Regex::new(r"^-S\d+$").unwrap();
                if re.is_match(rhs) {
                    out.push(Item::Op(OpKind::Neg));
                    return Ok(());
                }
            }

            // arithmetic: Sx = Sa + Sb
            {
                let re = Regex::new(r"^S\d+\s*([+\-*/%])\s*S\d+$" ).unwrap();
                if let Some(mm) = re.captures(rhs) {
                    let op = mm.get(1).unwrap().as_str();
                    let k = match op {
                        "+" => OpKind::Add,
                        "-" => OpKind::Sub,
                        "*" => OpKind::Mul,
                        "/" => OpKind::Div,
                        "%" => OpKind::Mod,
                        _ => bail!("unknown arithmetic op: {op}"),
                    };
                    out.push(Item::Op(k));
                    return Ok(());
                }
            }

            // bit_test: (Sa & Sb) ~= 0
            if rhs.contains('&') && rhs.contains("~= 0") {
                out.push(Item::Op(OpKind::BitTest));
                return Ok(());
            }

            // and/or (decompiler shape)
            if rhs.contains(" and ") && rhs.contains("~= nil") {
                out.push(Item::Op(OpKind::And));
                return Ok(());
            }
            if rhs.contains(" or ") && rhs.contains("~= nil") {
                out.push(Item::Op(OpKind::Or));
                return Ok(());
            }

            // comparisons
            {
                let re = Regex::new(r"^\(S\d+\s*(==|~=|>|<=|<|>=)\s*S\d+\)$" ).unwrap();
                if let Some(mm) = re.captures(rhs) {
                    let cop = mm.get(1).unwrap().as_str();
                    let k = match cop {
                        "==" => OpKind::SetE,
                        "~=" => OpKind::SetNe,
                        ">" => OpKind::SetG,
                        "<=" => OpKind::SetLe,
                        "<" => OpKind::SetL,
                        ">=" => OpKind::SetGe,
                        _ => bail!("unknown compare op: {cop}"),
                    };
                    out.push(Item::Op(k));
                    return Ok(());
                }
            }

            // RHS call: Sx = foo(...)
            {
                let re = Regex::new(r"^(\w+)\s*\((.*)\)\s*$" ).unwrap();
                if let Some(mm) = re.captures(rhs) {
                    let name = mm.get(1).unwrap().as_str();
                    let args_s = mm.get(2).unwrap().as_str().trim();
                    let _argc = if args_s.is_empty() {
                        0usize
                    } else {
                        args_s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).count()
                    };

                    if let Some(sid) = meta.syscall_id_by_name(name) {
                        out.push(Item::Op(OpKind::Syscall { id: sid }));
                        out.push(Item::Op(OpKind::PushReturn));
                        return Ok(());
                    }
                    if name.starts_with("f_0x") || user_fns.contains(name) {
                        out.push(Item::Op(OpKind::CallFn {
                            name: name.to_string(),
                        }));
                        out.push(Item::Op(OpKind::PushReturn));
                        return Ok(());
                    }
                    bail!("unknown callee: {name}");
                }
            }

            bail!("unsupported S assignment: {s}");
        }
    }

    // G[idx] = Sx
    {
        let re = Regex::new(r"^G\[(\d+)\]\s*=\s*S\d+\s*$" ).unwrap();
        if let Some(c) = re.captures(s) {
            let idx: u16 = c.get(1).unwrap().as_str().parse()?;
            out.push(Item::Op(OpKind::PopGlobal(idx)));
            return Ok(());
        }
    }

    // G[idx] = __ret
    {
        let re = Regex::new(r"^G\[(\d+)\]\s*=\s*__ret\s*$" ).unwrap();
        if let Some(c) = re.captures(s) {
            let idx: u16 = c.get(1).unwrap().as_str().parse()?;
            out.push(Item::Op(OpKind::PushReturn));
            out.push(Item::Op(OpKind::PopGlobal(idx)));
            return Ok(());
        }
    }

    // aN/lN = Sx
    {
        let re = Regex::new(r"^(a\d+|l\d+)\s*=\s*S\d+\s*$" ).unwrap();
        if let Some(c) = re.captures(s) {
            let var = c.get(1).unwrap().as_str();
            let idx = slot_to_stack_idx(var, args_count)?;
            out.push(Item::Op(OpKind::PopStack(idx)));
            return Ok(());
        }
    }

    // aN/lN = __ret
    {
        let re = Regex::new(r"^(a\d+|l\d+)\s*=\s*__ret\s*$" ).unwrap();
        if let Some(c) = re.captures(s) {
            let var = c.get(1).unwrap().as_str();
            let idx = slot_to_stack_idx(var, args_count)?;
            out.push(Item::Op(OpKind::PushReturn));
            out.push(Item::Op(OpKind::PopStack(idx)));
            return Ok(());
        }
    }

    // table store: GT[i][Skey] = Sval
    {
        let re = Regex::new(r"^GT\[(\d+)\]\[S\d+\]\s*=\s*S\d+\s*$" ).unwrap();
        if let Some(c) = re.captures(s) {
            let idx: u16 = c.get(1).unwrap().as_str().parse()?;
            out.push(Item::Op(OpKind::PopGlobalTable(idx)));
            return Ok(());
        }
    }

    // table store: LT[i][Skey] = Sval
    {
        let re = Regex::new(r"^LT\[(-?\d+)\]\[S\d+\]\s*=\s*S\d+\s*$" ).unwrap();
        if let Some(c) = re.captures(s) {
            let idx: i8 = c.get(1).unwrap().as_str().parse()?;
            out.push(Item::Op(OpKind::PopLocalTable(idx)));
            return Ok(());
        }
    }

    bail!("unsupported statement: {s}");
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

fn compile_stmts(
    stmts: &[Stmt],
    args_count: i8,
    meta: &Meta,
    user_fns: &HashSet<String>,
    out: &mut Vec<Item>,
    lg: &mut LabelGen,
    break_stack: &mut Vec<String>,
) -> Result<()> {
    for st in stmts {
        match st {
            Stmt::Simple(s) => compile_simple_stmt(s, args_count, meta, user_fns, out)?,
            Stmt::Return(None) => out.push(Item::Op(OpKind::Ret)),
            Stmt::Return(Some(expr)) => {
                // Return value is expected to already be on stack (as Sx), following the IR convention.
                // We support limited literals here for convenience.
                let e = expr.trim();
                if Regex::new(r"^S\d+$").unwrap().is_match(e) {
                    out.push(Item::Op(OpKind::Retv));
                } else if e == "nil" {
                    out.push(Item::Op(OpKind::PushNil));
                    out.push(Item::Op(OpKind::Retv));
                } else if e == "true" {
                    out.push(Item::Op(OpKind::PushTrue));
                    out.push(Item::Op(OpKind::Retv));
                } else if let Ok(v) = e.parse::<i64>() {
                    out.push(Item::Op(push_int(v)?));
                    out.push(Item::Op(OpKind::Retv));
                } else {
                    bail!("unsupported return expr: {e}");
                }
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
                let mut next_lbl = lg.fresh("if_next");

                for (idx, (cond, body)) in arms.iter().enumerate() {
                    let ck = parse_cond(cond);
                    let body_lbl = lg.fresh(&format!("if_body_{idx}"));
                    let after_lbl = if idx + 1 == arms.len() {
                        // Fallthrough to else or end.
                        next_lbl.clone()
                    } else {
                        lg.fresh(&format!("if_next_{idx}"))
                    };

                    match ck {
                        CondKind::AlwaysTrue => {
                            // Compile body and jump to end; ignore remaining arms.
                            compile_stmts(body, args_count, meta, user_fns, out, lg, break_stack)?;
                            out.push(Item::Op(OpKind::JmpLabel { label: end_lbl.clone() }));
                            next_lbl = after_lbl;
                            break;
                        }
                        CondKind::AlwaysFalse => {
                            // Skip to after_lbl.
                            out.push(Item::Op(OpKind::JmpLabel { label: after_lbl.clone() }));
                        }
                        CondKind::NonZero => {
                            out.push(Item::Op(OpKind::JzLabel {
                                label: after_lbl.clone(),
                            }));
                            compile_stmts(body, args_count, meta, user_fns, out, lg, break_stack)?;
                            out.push(Item::Op(OpKind::JmpLabel { label: end_lbl.clone() }));
                        }
                        CondKind::Zero => {
                            out.push(Item::Op(OpKind::JzLabel {
                                label: body_lbl.clone(),
                            }));
                            out.push(Item::Op(OpKind::JmpLabel {
                                label: after_lbl.clone(),
                            }));
                            out.push(Item::Label(Label::new(body_lbl)));
                            compile_stmts(body, args_count, meta, user_fns, out, lg, break_stack)?;
                            out.push(Item::Op(OpKind::JmpLabel { label: end_lbl.clone() }));
                        }
                    }

                    out.push(Item::Label(Label::new(after_lbl.clone())));
                    next_lbl = after_lbl;
                }

                if let Some(eb) = else_arm {
                    compile_stmts(eb, args_count, meta, user_fns, out, lg, break_stack)?;
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
                        // No condition check.
                        compile_stmts(body, args_count, meta, user_fns, out, lg, break_stack)?;
                        out.push(Item::Op(OpKind::JmpLabel { label: head }));
                    }
                    CondKind::AlwaysFalse => {
                        out.push(Item::Op(OpKind::JmpLabel { label: end.clone() }));
                    }
                    CondKind::NonZero => {
                        out.push(Item::Op(OpKind::JzLabel { label: end.clone() }));
                        compile_stmts(body, args_count, meta, user_fns, out, lg, break_stack)?;
                        out.push(Item::Op(OpKind::JmpLabel { label: head }));
                    }
                    CondKind::Zero => {
                        out.push(Item::Op(OpKind::JzLabel { label: body_lbl.clone() }));
                        out.push(Item::Op(OpKind::JmpLabel { label: end.clone() }));
                        out.push(Item::Label(Label::new(body_lbl)));
                        compile_stmts(body, args_count, meta, user_fns, out, lg, break_stack)?;
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
    // Typical decompiler pattern:
    //   local __pc = 0
    //   while true do
    //     if __pc == 0 then
    //       ...
    //     elseif __pc == 1 then
    //       ...
    //     else
    //       return
    //     end
    //   end
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

        // Drop pure local declarations in a case.
        if t.starts_with("local ") && !t.contains('=') {
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("local ") {
            t = rest.trim().to_string();
        }

        // return / return <expr>
        if t == "return" {
            out.push(Item::Op(OpKind::Ret));
            return Ok(());
        }

        // Some decompiler variants emit `return Sx` for single-value returns.
        // In this IR, `Sx` is expected to be the current top-of-stack, so we can
        // directly emit `retv` (pop one value and return it).
        if let Some(rest) = t.strip_prefix("return ") {
            let e = rest.trim();

            if Regex::new(r"^S\d+$").unwrap().is_match(e) {
                out.push(Item::Op(OpKind::Retv));
                return Ok(());
            }

            // Literal returns (convenience)
            if e == "nil" || e == "false" {
                out.push(Item::Op(OpKind::PushNil));
                out.push(Item::Op(OpKind::Retv));
                return Ok(());
            }
            if e == "true" {
                out.push(Item::Op(OpKind::PushTrue));
                out.push(Item::Op(OpKind::Retv));
                return Ok(());
            }
            if let Ok(v) = e.parse::<i64>() {
                out.push(Item::Op(push_int(v)?));
                out.push(Item::Op(OpKind::Retv));
                return Ok(());
            }
            if let Ok(vf) = e.parse::<f32>() {
                // Distinguish integer vs float by presence of '.' or exponent.
                if e.contains('.') || e.contains('e') || e.contains('E') {
                    out.push(Item::Op(OpKind::PushF32(vf)));
                    out.push(Item::Op(OpKind::Retv));
                    return Ok(());
                }
            }
            if let Some(mm) = Regex::new(r#"^\"(.*)\"$"#).unwrap().captures(e) {
                let lit = mm.get(1).unwrap().as_str();
                out.push(Item::Op(OpKind::PushString(lua_unescape_string(lit))));
                out.push(Item::Op(OpKind::Retv));
                return Ok(());
            }

            bail!("unsupported return expr in pc bb {pc}: {e}");
        }

        // Unconditional jump via __pc = N
        if let Some(c) = re_pc_set.captures(&t) {
            let target: u32 = c.get(1).unwrap().as_str().parse()?;
            out.push(Item::Op(OpKind::JmpLabel {
                label: bb_label(fn_name, target),
            }));
            return Ok(());
        }

        // Terminator: if Sx (==|~=) 0 then __pc=A else __pc=B end
        if let Some(c) = re_term_if.captures(&t) {
            let op = c.get(1).unwrap().as_str(); // == or ~=
            // then: __pc = A
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

            // Encode with the VM's "jz" (jump on zero).
            // if Sx == 0: jz then_pc else jmp else_pc
            // if Sx ~= 0: jz else_pc else jmp then_pc
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

        // Normal statement inside bb.
        compile_simple_stmt(&t, args_count, meta, user_fns, out)?;
        i += 1;
    }

    // Conservative fallback.
    out.push(Item::Op(OpKind::Ret));
    Ok(())
}

fn compile_pc_dispatcher_function(
    f: &Function,
    meta: &Meta,
    user_fns: &HashSet<String>,
    out: &mut Vec<Item>,
) -> Result<()> {
    if f.raw.len() < 2 {
        bail!("function {}: too short", f.name);
    }
    let body: Vec<String> = f.raw[1..f.raw.len() - 1].to_vec();

    let entry_pc = parse_entry_pc(&body);

    let re_case = Regex::new(r"^(if|elseif)\s+__pc\s*==\s*(\d+)\s+then\s*$").unwrap();

    // Find the first dispatcher case.
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

    // Emit cases with the entry case first (avoid an extra jmp at function start).
    if let Some(pos) = cases.iter().position(|(pc, _)| *pc == entry_pc) {
        if pos != 0 {
            let entry = cases.remove(pos);
            cases.insert(0, entry);
        }
    }

    for (pc, lines) in cases {
        compile_pc_case(pc, &lines, &f.name, f.args_count, meta, user_fns, out)?;
    }

    Ok(())
}

pub fn compile_program(meta: &Meta, funs: &[Function]) -> Result<Vec<Item>> {
    let mut items: Vec<Item> = Vec::new();

    let user_fns: HashSet<String> = funs.iter().map(|f| f.name.clone()).collect();

    for f in funs {
        items.push(Item::Label(Label::new(format!("fn:{}", f.name))));
        items.push(Item::Op(OpKind::InitStack {
            args: f.args_count,
            locals: f.locals_count,
        }));

        if looks_like_pc_dispatcher(&f.raw) {
            compile_pc_dispatcher_function(f, meta, &user_fns, &mut items)?;
        } else {
            let mut lg = LabelGen::new(format!("fn:{}", f.name));
            let mut break_stack: Vec<String> = Vec::new();
            compile_stmts(
                &f.body,
                f.args_count,
                meta,
                &user_fns,
                &mut items,
                &mut lg,
                &mut break_stack,
            )?;
        }

        // Ensure function ends with a ret. If the source omitted explicit return, add Ret.
        if !matches!(items.last(), Some(Item::Op(OpKind::Ret | OpKind::Retv))) {
            items.push(Item::Op(OpKind::Ret));
        }
    }

    Ok(items)
}
