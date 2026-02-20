use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::cfg::BlockTerm;
use crate::decode::{Instruction, Op};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    And,
    Or,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Nil,
    Bool(bool),
    Int(i64),
    Float(String),
    Str(String),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Index(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn var(name: impl Into<String>) -> Self {
        Expr::Var(name.into())
    }

    pub fn stack_var(idx: usize) -> Self {
        Expr::Var(format!("S{}", idx))
    }

    pub fn global_var() -> Self {
        Expr::Var("G".to_string())
    }

    pub fn global_table_var() -> Self {
        Expr::Var("GT".to_string())
    }

    pub fn local_table_var() -> Self {
        Expr::Var("LT".to_string())
    }

    pub fn index(base: Expr, idx: Expr) -> Self {
        Expr::Index(Box::new(base), Box::new(idx))
    }

    pub fn unary(op: UnOp, a: Expr) -> Self {
        match (&op, &a) {
            (UnOp::Neg, Expr::Int(v)) => Expr::Int(-v),
            _ => Expr::Unary(op, Box::new(a)),
        }
    }

    pub fn binary(op: BinOp, a: Expr, b: Expr) -> Self {
        // Constant folding (safe subset).
        match (&op, &a, &b) {
            (BinOp::Add, Expr::Int(x), Expr::Int(y)) => return Expr::Int(x + y),
            (BinOp::Sub, Expr::Int(x), Expr::Int(y)) => return Expr::Int(x - y),
            (BinOp::Mul, Expr::Int(x), Expr::Int(y)) => return Expr::Int(x * y),
            (BinOp::Eq, Expr::Int(x), Expr::Int(y)) => return Expr::Bool(x == y),
            (BinOp::Ne, Expr::Int(x), Expr::Int(y)) => return Expr::Bool(x != y),
            (BinOp::Gt, Expr::Int(x), Expr::Int(y)) => return Expr::Bool(x > y),
            (BinOp::Ge, Expr::Int(x), Expr::Int(y)) => return Expr::Bool(x >= y),
            (BinOp::Lt, Expr::Int(x), Expr::Int(y)) => return Expr::Bool(x < y),
            (BinOp::Le, Expr::Int(x), Expr::Int(y)) => return Expr::Bool(x <= y),

            // nil comparisons
            (BinOp::Eq, Expr::Nil, Expr::Nil) => return Expr::Bool(true),
            (BinOp::Ne, Expr::Nil, Expr::Nil) => return Expr::Bool(false),
            (BinOp::Eq, Expr::Nil, _) | (BinOp::Eq, _, Expr::Nil) => {
                // If the other side is a known non-nil literal.
                if matches!(a, Expr::Nil) {
                    if matches!(b, Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Str(_)) {
                        return Expr::Bool(false);
                    }
                }
                if matches!(b, Expr::Nil) {
                    if matches!(a, Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Str(_)) {
                        return Expr::Bool(false);
                    }
                }
            }
            (BinOp::Ne, Expr::Nil, _) | (BinOp::Ne, _, Expr::Nil) => {
                if matches!(a, Expr::Nil) {
                    if matches!(b, Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Str(_)) {
                        return Expr::Bool(true);
                    }
                }
                if matches!(b, Expr::Nil) {
                    if matches!(a, Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Str(_)) {
                        return Expr::Bool(true);
                    }
                }
            }

            // Simple algebraic identities that do not drop evaluation.
            (BinOp::Add, x, Expr::Int(0)) => return x.clone(),
            (BinOp::Add, Expr::Int(0), x) => return x.clone(),
            (BinOp::Sub, x, Expr::Int(0)) => return x.clone(),
            (BinOp::Mul, x, Expr::Int(1)) => return x.clone(),
            (BinOp::Mul, Expr::Int(1), x) => return x.clone(),

            _ => {}
        }
        Expr::Binary(op, Box::new(a), Box::new(b))
    }

    fn precedence(&self) -> u8 {
        match self {
            Expr::Nil
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Var(_)
            | Expr::Index(_, _) => 100,
            Expr::Unary(_, _) => 90,
            Expr::Binary(op, _, _) => match op {
                BinOp::Mul | BinOp::Div | BinOp::Mod => 80,
                BinOp::Add | BinOp::Sub => 70,
                BinOp::BitAnd => 65,
                BinOp::Eq | BinOp::Ne | BinOp::Gt | BinOp::Ge | BinOp::Lt | BinOp::Le => 50,
                BinOp::And => 40,
                BinOp::Or => 30,
            },
        }
    }

    fn mark_stack_var(name: &str, used_s: &mut BTreeSet<usize>) {
        if let Some(rest) = name.strip_prefix('S') {
            if !rest.is_empty() && rest.bytes().all(|c| c.is_ascii_digit()) {
                if let Ok(v) = rest.parse::<usize>() {
                    used_s.insert(v);
                }
            }
        }
    }

    pub fn render(&self, used_s: &mut BTreeSet<usize>, parent_prec: u8) -> String {
        let my_prec = self.precedence();
        let mut s = match self {
            Expr::Nil => "nil".to_string(),
            Expr::Bool(v) => {
                if *v {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            Expr::Int(v) => v.to_string(),
            Expr::Float(v) => v.clone(),
            Expr::Str(v) => {
                let lit = v.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", lit)
            }
            Expr::Var(v) => {
                Self::mark_stack_var(v, used_s);
                v.clone()
            }
            Expr::Index(base, idx) => {
                let b = base.render(used_s, 100);
                let i = idx.render(used_s, 0);
                format!("{}[{}]", b, i)
            }
            Expr::Unary(UnOp::Neg, a) => {
                let aa = a.render(used_s, my_prec);
                format!("-{}", aa)
            }
            Expr::Binary(op, a, b) => {
                let aa = a.render(used_s, my_prec);
                let bb = b.render(used_s, my_prec + 1);
                let op_s = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::BitAnd => "&",
                    BinOp::And => "and",
                    BinOp::Or => "or",
                    BinOp::Eq => "==",
                    BinOp::Ne => "~=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                };
                if matches!(op, BinOp::And | BinOp::Or) {
                    format!("({}) {} ({})", aa, op_s, bb)
                } else {
                    format!("{} {} {}", aa, op_s, bb)
                }
            }
        };

        if my_prec < parent_prec {
            s = format!("({})", s);
        }
        s
    }

    pub fn const_eq_zero(&self) -> Option<bool> {
        match self {
            Expr::Int(v) => Some(*v == 0),
            // Do NOT treat booleans/nil as numeric; the generated code uses `cond == 0`.
            Expr::Float(s) => s.parse::<f64>().ok().map(|v| v == 0.0),
            _ => None,
        }
    }
}

pub struct BlockEmitter<'a> {
    indent: &'a str,
    func_args: u8,
    callee_args: &'a BTreeMap<u32, u8>,
    used_s: &'a mut BTreeSet<usize>,
    out: String,
    stack: Vec<Expr>,
}

impl<'a> BlockEmitter<'a> {
    pub fn new(
        indent: &'a str,
        func_args: u8,
        callee_args: &'a BTreeMap<u32, u8>,
        used_s: &'a mut BTreeSet<usize>,
    ) -> Self {
        BlockEmitter {
            indent,
            func_args,
            callee_args,
            used_s,
            out: String::new(),
            stack: Vec::new(),
        }
    }

    pub fn init_stack(&mut self, depth: usize) {
        self.stack.clear();
        for i in 0..depth {
            self.stack.push(Expr::stack_var(i));
        }
    }

    pub fn take_output(self) -> String {
        self.out
    }

    fn emit_line(&mut self, line: &str) {
        let _ = writeln!(&mut self.out, "{}{}", self.indent, line);
    }

    fn pop(&mut self) -> Expr {
        self.stack.pop().unwrap_or(Expr::Nil)
    }

    fn push(&mut self, e: Expr) {
        self.stack.push(e);
    }

    fn stack_slot_get(&self, idx: i8) -> String {
        // This is *frame* stack (args + locals), not the operand stack.
        if idx < 0 {
            let abs = (-idx) as u8 - 2;
            if abs <= self.func_args {
                let a = (self.func_args - abs) as usize;
                return format!("a{}", a);
            }
            return format!("a_{}", idx);
        }

        let u = idx as u8;
        if u < self.func_args {
            format!("a{}", u as usize)
        } else {
            let l = (u - self.func_args) as usize;
            format!("l{}", l)
        }
    }

    fn materialize_all_stack_slots(&mut self) {
        // Ensure S0..S{n-1} hold the stack values for successor blocks.
        for i in 0..self.stack.len() {
            let want = Expr::stack_var(i);
            if self.stack[i] != want {
                let rhs = self.stack[i].render(self.used_s, 0);
                self.used_s.insert(i);
                self.emit_line(&format!("S{} = {}", i, rhs));
                self.stack[i] = Expr::stack_var(i);
            } else {
                // Mark it as used only if it is referenced later; render() will do that.
            }
        }
    }

    pub fn emit_inst(&mut self, inst: &Instruction) {
        match &inst.op {
            Op::Nop | Op::InitStack { .. } => {}

            Op::PushNil => self.push(Expr::Nil),
            Op::PushTrue => self.push(Expr::Bool(true)),
            Op::PushI8(v) => self.push(Expr::Int(*v as i64)),
            Op::PushI16(v) => self.push(Expr::Int(*v as i64)),
            Op::PushI32(v) => self.push(Expr::Int(*v as i64)),
            Op::PushF32(v) => self.push(Expr::Float(format!("{}", v))),
            Op::PushString(s) => self.push(Expr::Str(s.clone())),

            Op::PushTop => {
                if let Some(top) = self.stack.last().cloned() {
                    self.push(top);
                } else {
                    self.push(Expr::Nil);
                }
            }
            Op::PushReturn => self.push(Expr::var("__ret")),

            Op::PushGlobal(idx) => {
                let e = Expr::index(Expr::global_var(), Expr::Int(*idx as i64));
                self.push(e);
            }
            Op::PopGlobal(idx) => {
                let v = self.pop();
                let rhs = v.render(self.used_s, 0);
                self.emit_line(&format!("G[{}] = {}", idx, rhs));
            }

            Op::PushStack(idx) => {
                let v = self.stack_slot_get(*idx);
                self.push(Expr::var(v));
            }
            Op::PopStack(idx) => {
                let rhs_expr = self.pop();
                let rhs = rhs_expr.render(self.used_s, 0);
                let lhs = self.stack_slot_get(*idx);
                self.emit_line(&format!("{} = {}", lhs, rhs));
            }

            Op::PushGlobalTable(idx) => {
                // top = GT[idx][top]
                if let Some(last) = self.stack.pop() {
                    let base = Expr::index(Expr::global_table_var(), Expr::Int(*idx as i64));
                    let e = Expr::index(base, last);
                    self.stack.push(e);
                } else {
                    self.stack.push(Expr::Nil);
                }
            }
            Op::PopGlobalTable(idx) => {
                // GT[idx][key] = value
                let v = self.pop();
                let k = self.pop();
                let base = Expr::index(Expr::global_table_var(), Expr::Int(*idx as i64));
                let lhs = Expr::index(base, k).render(self.used_s, 0);
                let rhs = v.render(self.used_s, 0);
                self.emit_line(&format!("{} = {}", lhs, rhs));
            }

            Op::PushLocalTable(idx) => {
                if let Some(last) = self.stack.pop() {
                    let base = Expr::index(Expr::local_table_var(), Expr::Int(*idx as i64));
                    let e = Expr::index(base, last);
                    self.stack.push(e);
                } else {
                    self.stack.push(Expr::Nil);
                }
            }
            Op::PopLocalTable(idx) => {
                let v = self.pop();
                let k = self.pop();
                let base = Expr::index(Expr::local_table_var(), Expr::Int(*idx as i64));
                let lhs = Expr::index(base, k).render(self.used_s, 0);
                let rhs = v.render(self.used_s, 0);
                self.emit_line(&format!("{} = {}", lhs, rhs));
            }

            Op::Neg => {
                let a = self.pop();
                self.push(Expr::unary(UnOp::Neg, a));
            }

            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::BitTest | Op::And | Op::Or
            | Op::SetE | Op::SetNE | Op::SetG | Op::SetGE | Op::SetL | Op::SetLE => {
                let b = self.pop();
                let a = self.pop();
                let e = match &inst.op {
                    Op::Add => Expr::binary(BinOp::Add, a, b),
                    Op::Sub => Expr::binary(BinOp::Sub, a, b),
                    Op::Mul => Expr::binary(BinOp::Mul, a, b),
                    Op::Div => Expr::binary(BinOp::Div, a, b),
                    Op::Mod => Expr::binary(BinOp::Mod, a, b),
                    Op::BitTest => {
                        let and = Expr::binary(BinOp::BitAnd, a, b);
                        Expr::binary(BinOp::Ne, and, Expr::Int(0))
                    }
                    Op::And => {
                        let aa = Expr::binary(BinOp::Ne, a, Expr::Nil);
                        let bb = Expr::binary(BinOp::Ne, b, Expr::Nil);
                        Expr::binary(BinOp::And, aa, bb)
                    }
                    Op::Or => {
                        let aa = Expr::binary(BinOp::Ne, a, Expr::Nil);
                        let bb = Expr::binary(BinOp::Ne, b, Expr::Nil);
                        Expr::binary(BinOp::Or, aa, bb)
                    }
                    Op::SetE => Expr::binary(BinOp::Eq, a, b),
                    Op::SetNE => Expr::binary(BinOp::Ne, a, b),
                    Op::SetG => Expr::binary(BinOp::Gt, a, b),
                    Op::SetGE => Expr::binary(BinOp::Ge, a, b),
                    Op::SetL => Expr::binary(BinOp::Lt, a, b),
                    Op::SetLE => Expr::binary(BinOp::Le, a, b),
                    _ => Expr::Nil,
                };
                self.push(e);
            }

            Op::Call { target } => {
                let argc = self.callee_args.get(target).copied().unwrap_or(0) as usize;
                if self.stack.len() < argc {
                    self.emit_line(&format!(
                        "-- call f_{:08X} argc={} on short stack",
                        target, argc
                    ));
                    self.emit_line(&format!("__ret = f_{:08X}()", target));
                    self.stack.clear();
                    return;
                }

                let base = self.stack.len() - argc;
                let mut args_s = String::new();
                for i in 0..argc {
                    if i > 0 {
                        args_s.push_str(", ");
                    }
                    let a = self.stack[base + i].clone();
                    args_s.push_str(&a.render(self.used_s, 0));
                }
                self.emit_line(&format!("__ret = f_{:08X}({})", target, args_s));
                self.stack.truncate(base);
            }

            Op::Syscall { name, args, id } => {
                let argc = *args as usize;
                if self.stack.len() < argc {
                    self.emit_line(&format!(
                        "-- syscall {} (id={}) argc={} on short stack",
                        name, id, argc
                    ));
                    self.emit_line(&format!("__ret = {}()", name));
                    self.stack.clear();
                    return;
                }

                let base = self.stack.len() - argc;
                let mut args_s = String::new();
                for i in 0..argc {
                    if i > 0 {
                        args_s.push_str(", ");
                    }
                    let a = self.stack[base + i].clone();
                    args_s.push_str(&a.render(self.used_s, 0));
                }
                self.emit_line(&format!("__ret = {}({})", name, args_s));
                self.stack.truncate(base);
            }

            // Control-flow is handled at the basic-block terminator level.
            Op::Jmp { .. } | Op::Jz { .. } | Op::Ret | Op::RetV => {}

            Op::Unknown(opcode) => {
                self.emit_line(&format!("-- unknown opcode 0x{:02X}", opcode));
            }
        }
    }

    pub fn emit_terminator(&mut self, term: BlockTerm, succs: &[usize]) {
        match term {
            BlockTerm::Ret => {
                self.emit_line("return");
            }
            BlockTerm::RetV => {
                let v = self.pop();
                let rhs = v.render(self.used_s, 0);
                self.emit_line(&format!("return {}", rhs));
            }
            BlockTerm::Jmp | BlockTerm::Fallthrough => {
                self.materialize_all_stack_slots();
                if let Some(&t) = succs.get(0) {
                    self.emit_line(&format!("__pc = {}", t));
                } else {
                    self.emit_line("return");
                }
            }
            BlockTerm::Jz => {
                let cond = self.pop();
                self.materialize_all_stack_slots();
                let t = succs.get(0).copied();
                let f = succs.get(1).copied();

                if let Some(is_zero) = cond.const_eq_zero() {
                    // Fold constant condition.
                    let dst = if is_zero { t } else { f };
                    match dst {
                        Some(id) => self.emit_line(&format!("__pc = {}", id)),
                        None => self.emit_line("return"),
                    }
                    return;
                }

                let c = cond.render(self.used_s, 0);
                self.emit_line(&format!("if {} == 0 then", c));
                match t {
                    Some(tid) => self.emit_line(&format!("  __pc = {}", tid)),
                    None => self.emit_line("  return"),
                }
                self.emit_line("else");
                match f {
                    Some(fid) => self.emit_line(&format!("  __pc = {}", fid)),
                    None => self.emit_line("  return"),
                }
                self.emit_line("end");
            }
        }
    }
}
