use std::cmp::Ordering;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use byteorder::{ByteOrder, LittleEndian};

use crate::variant::Variant;

use super::opcode::Opcode;

pub type SyscallId = u16;

/// VM runtime hooks.
///
/// This keeps the VM independent from the engine crate while still enabling:
/// - syscalls (engine integration)
/// - table operations (global/local tables)
pub trait VmRuntime {
    /// Query how many arguments a syscall expects.
    fn syscall_argc(&self, id: SyscallId) -> Option<usize>;

    /// Dispatch syscall.
    fn syscall_call(&mut self, id: SyscallId, args: &[Variant]) -> Result<Variant>;

    /// Optional: table lookup (default: returns Nil).
    fn table_get(&mut self, _table_id: u32, _key: &Variant) -> Result<Variant> {
        Ok(Variant::Nil)
    }

    /// Optional: table set (default: no-op).
    fn table_set(&mut self, _table_id: u32, _key: Variant, _value: Variant) -> Result<()> {
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum VmError {
    #[error("pc out of range: pc=0x{pc:X}, bytecode_len=0x{len:X}")]
    PcOutOfRange { pc: u32, len: u32 },

    #[error("invalid opcode: 0x{opcode:02X} at pc=0x{pc:X}")]
    InvalidOpcode { opcode: u8, pc: u32 },

    #[error("stack underflow")]
    StackUnderflow,

    #[error("stack overflow (limit={limit})")]
    StackOverflow { limit: usize },

    #[error("call stack underflow")]
    CallStackUnderflow,

    #[error("syscall id={id} has no argc information")]
    SyscallArgcMissing { id: u16 },

    #[error("syscall id={id} failed: {msg}")]
    SyscallFailed { id: u16, msg: String },

    #[error("global index out of range: idx={idx}, globals_len={len}")]
    GlobalOob { idx: usize, len: usize },
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub return_pc: u32,
    pub prev_stack_base: usize,

    /// The caller stack length before pushing arguments for this call.
    /// This is set by `InitStack` once `argc` is known.
    pub prev_stack_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecOutcome {
    /// Executed one opcode and should continue.
    Step,
    /// Returned from the root procedure (no more frames).
    Halt,
    /// Yielded / cooperatively paused.
    Yield,
}

/// A single VM thread/coroutine.
#[derive(Clone, Debug)]
pub struct ThreadContext {
    bytecode: Arc<[u8]>,
    pub pc: u32,

    /// Operand stack (Variants).
    pub stack: Vec<Variant>,

    /// Base index for local variables of the current frame.
    pub stack_base: usize,

    /// A separate call stack (keeps stack layout simple and robust).
    pub call_stack: Vec<Frame>,

    /// Return value slot used by `retv` and `push_ret_value`.
    pub return_value: Variant,

    pub should_break: bool,
    pub status: u32,
}

impl ThreadContext {
    pub fn new(bytecode: Arc<[u8]>, entry_pc: u32) -> Self {
        Self {
            bytecode,
            pc: entry_pc,
            stack: Vec::with_capacity(256),
            stack_base: 0,
            call_stack: Vec::new(),
            return_value: Variant::Nil,
            should_break: false,
            status: 0,
        }
    }

    pub fn bytecode(&self) -> &[u8] {
        &self.bytecode
    }

    fn ensure_pc(&self, need: u32) -> std::result::Result<(), VmError> {
        let len = self.bytecode.len() as u32;
        if self.pc.saturating_add(need) > len {
            return Err(VmError::PcOutOfRange { pc: self.pc, len });
        }
        Ok(())
    }

    fn read_u8(&mut self) -> std::result::Result<u8, VmError> {
        self.ensure_pc(1)?;
        let b = self.bytecode[self.pc as usize];
        self.pc += 1;
        Ok(b)
    }

    fn read_i8(&mut self) -> std::result::Result<i8, VmError> {
        Ok(self.read_u8()? as i8)
    }

    fn read_u16(&mut self) -> std::result::Result<u16, VmError> {
        self.ensure_pc(2)?;
        let off = self.pc as usize;
        let v = LittleEndian::read_u16(&self.bytecode[off..off + 2]);
        self.pc += 2;
        Ok(v)
    }

    fn read_i16(&mut self) -> std::result::Result<i16, VmError> {
        Ok(self.read_u16()? as i16)
    }

    fn read_u32(&mut self) -> std::result::Result<u32, VmError> {
        self.ensure_pc(4)?;
        let off = self.pc as usize;
        let v = LittleEndian::read_u32(&self.bytecode[off..off + 4]);
        self.pc += 4;
        Ok(v)
    }

    fn pop(&mut self) -> std::result::Result<Variant, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    fn peek_mut(&mut self) -> std::result::Result<&mut Variant, VmError> {
        self.stack.last_mut().ok_or(VmError::StackUnderflow)
    }

    fn push(&mut self, v: Variant) -> std::result::Result<(), VmError> {
        if self.stack.len() >= 256 {
            return Err(VmError::StackOverflow { limit: 256 });
        }
        self.stack.push(v);
        Ok(())
    }

    fn cmp_variant(&self, a: &Variant, b: &Variant) -> Option<Ordering> {
        match (a, b) {
            (Variant::Int(x), Variant::Int(y)) => Some(x.cmp(y)),
            (Variant::Int(x), Variant::Float(y)) => ( (*x as f32).partial_cmp(y) ),
            (Variant::Float(x), Variant::Int(y)) => x.partial_cmp(&(*y as f32)),
            (Variant::Float(x), Variant::Float(y)) => x.partial_cmp(y),

            (Variant::DynStr(x), Variant::DynStr(y)) => Some(x.cmp(y)),

            (Variant::ConstStr { off: aoff, len: alen }, Variant::ConstStr { off: boff, len: blen }) => {
                let aoff = *aoff as usize;
                let boff = *boff as usize;
                let alen = *alen as usize;
                let blen = *blen as usize;
                if aoff + alen <= self.bytecode.len() && boff + blen <= self.bytecode.len() {
                    let aa = &self.bytecode[aoff..aoff + alen];
                    let bb = &self.bytecode[boff..boff + blen];
                    Some(aa.cmp(bb))
                } else {
                    None
                }
            }

            _ => None,
        }
    }

    /// Execute a single opcode.
    pub fn step<R: VmRuntime>(&mut self, runtime: &mut R, globals: &mut [Variant]) -> std::result::Result<ExecOutcome, VmError> {
        self.ensure_pc(1)?;
        let op_b = self.read_u8()?;
        let op = Opcode::decode(op_b).ok_or(VmError::InvalidOpcode { opcode: op_b, pc: self.pc - 1 })?;

        use Opcode::*;
        match op {
            Nop => Ok(ExecOutcome::Step),

            InitStack => {
                // args_cnt: u8, local_cnt: i8
                let argc = self.read_u8()? as usize;
                let local_cnt = self.read_i8()? as i32;
                let local_cnt = if local_cnt < 0 { 0 } else { local_cnt as usize };

                if self.stack.len() < argc {
                    return Err(VmError::StackUnderflow);
                }
                let args_start = self.stack.len() - argc;

                // Ensure there is a current frame (root entry may not have one yet).
                if self.call_stack.is_empty() {
                    self.call_stack.push(Frame {
                        return_pc: 0,
                        prev_stack_base: 0,
                        prev_stack_len: args_start,
                    });
                } else {
                    // Fill prev_stack_len for the current frame (created by Call).
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.prev_stack_len = args_start;
                }

                self.stack_base = args_start;

                for _ in 0..local_cnt {
                    self.push(Variant::Nil)?;
                }

                Ok(ExecOutcome::Step)
            }

            Call => {
                let target = self.read_u32()?;
                let return_pc = self.pc;

                // Create a new frame; `prev_stack_len` will be filled by callee's InitStack.
                self.call_stack.push(Frame {
                    return_pc,
                    prev_stack_base: self.stack_base,
                    prev_stack_len: 0,
                });

                self.pc = target;
                Ok(ExecOutcome::Step)
            }

            Syscall => {
                let id = self.read_u16()? as SyscallId;
                let argc = runtime.syscall_argc(id).ok_or(VmError::SyscallArgcMissing { id })?;
                if self.stack.len() < argc {
                    return Err(VmError::StackUnderflow);
                }
                let start = self.stack.len() - argc;
                let args = &self.stack[start..];

                self.return_value = Variant::Nil;
                let rv = runtime
                    .syscall_call(id, args)
                    .map_err(|e| VmError::SyscallFailed { id, msg: format!("{e:#}") })?;
                self.return_value = rv;

                self.stack.truncate(start);
                Ok(ExecOutcome::Step)
            }

            Ret => {
                self.return_value = Variant::Nil;
                self.ret_common()
            }

            RetV => {
                let v = self.pop()?;
                self.return_value = v;
                self.ret_common()
            }

            Jmp => {
                let target = self.read_u32()?;
                self.pc = target;
                Ok(ExecOutcome::Step)
            }

            Jz => {
                let v = self.pop()?;
                let target = self.read_u32()?;
                if !v.truthy() {
                    self.pc = target;
                }
                Ok(ExecOutcome::Step)
            }

            PushNil => self.push(Variant::Nil).map(|_| ExecOutcome::Step),

            PushTrue => self.push(Variant::Bool(true)).map(|_| ExecOutcome::Step),

            PushI32 => {
                let raw = self.read_u32()? as i32;
                self.push(Variant::Int(raw)).map(|_| ExecOutcome::Step)
            }

            PushI16 => {
                let v = self.read_i16()? as i32;
                self.push(Variant::Int(v)).map(|_| ExecOutcome::Step)
            }

            PushI8 => {
                let v = self.read_i8()? as i32;
                self.push(Variant::Int(v)).map(|_| ExecOutcome::Step)
            }

            PushF32 => {
                let raw = self.read_u32()?;
                let v = f32::from_bits(raw);
                self.push(Variant::Float(v)).map(|_| ExecOutcome::Step)
            }

            PushStr => {
                let len = self.read_u8()?;
                let off = self.pc;
                self.ensure_pc(len as u32)?;
                self.pc += len as u32;
                self.push(Variant::ConstStr { off, len }).map(|_| ExecOutcome::Step)
            }

            PushGlobal => {
                let idx = self.read_i16()? as i32;
                if idx < 0 {
                    return Err(VmError::GlobalOob { idx: idx as usize, len: globals.len() });
                }
                let idx = idx as usize;
                if idx >= globals.len() {
                    return Err(VmError::GlobalOob { idx, len: globals.len() });
                }
                self.push(globals[idx].clone()).map(|_| ExecOutcome::Step)
            }

            PushStack => {
                let off = self.read_u8()? as usize;
                let idx = self.stack_base + off;
                if idx >= self.stack.len() {
                    return Err(VmError::StackUnderflow);
                }
                let v = self.stack[idx].clone();
                self.push(v).map(|_| ExecOutcome::Step)
            }

            PushGlobalTable => {
                let idx = self.read_i16()? as i32;
                let key = self.pop()?;
                if idx < 0 {
                    self.push(Variant::Nil)?;
                    return Ok(ExecOutcome::Step);
                }
                let idx = idx as usize;
                if idx >= globals.len() {
                    self.push(Variant::Nil)?;
                    return Ok(ExecOutcome::Step);
                }
                let table_id = match &globals[idx] {
                    Variant::Table(tid) => *tid,
                    _ => {
                        self.push(Variant::Nil)?;
                        return Ok(ExecOutcome::Step);
                    }
                };
                let v = runtime.table_get(table_id, &key).map_err(|e| VmError::SyscallFailed { id: 0xFFFF, msg: format!("{e:#}") })?;
                self.push(v)?;
                Ok(ExecOutcome::Step)
            }

            PushLocalTable => {
                let local_off = self.read_u8()? as usize;
                let key = self.pop()?;
                let idx = self.stack_base + local_off;
                if idx >= self.stack.len() {
                    self.push(Variant::Nil)?;
                    return Ok(ExecOutcome::Step);
                }
                let table_id = match &self.stack[idx] {
                    Variant::Table(tid) => *tid,
                    _ => {
                        self.push(Variant::Nil)?;
                        return Ok(ExecOutcome::Step);
                    }
                };
                let v = runtime.table_get(table_id, &key).map_err(|e| VmError::SyscallFailed { id: 0xFFFF, msg: format!("{e:#}") })?;
                self.push(v)?;
                Ok(ExecOutcome::Step)
            }

            PushTop => {
                let v = self.stack.last().cloned().unwrap_or(Variant::Nil);
                self.push(v).map(|_| ExecOutcome::Step)
            }

            PushRetValue => {
                let v = std::mem::replace(&mut self.return_value, Variant::Nil);
                self.push(v).map(|_| ExecOutcome::Step)
            }

            PopGlobal => {
                let idx = self.read_i16()? as i32;
                let v = self.pop()?;
                if idx < 0 {
                    return Ok(ExecOutcome::Step);
                }
                let idx = idx as usize;
                if idx >= globals.len() {
                    return Err(VmError::GlobalOob { idx, len: globals.len() });
                }
                globals[idx] = v;
                Ok(ExecOutcome::Step)
            }

            LocalCopy => {
                let off = self.read_u8()? as usize;
                let v = self.pop()?;
                let idx = self.stack_base + off;
                if idx >= self.stack.len() {
                    return Err(VmError::StackUnderflow);
                }
                self.stack[idx] = v;
                Ok(ExecOutcome::Step)
            }

            PopGlobalTable => {
                let idx = self.read_i16()? as i32;
                let value = self.pop()?;
                let key = self.pop()?;
                if idx < 0 {
                    return Ok(ExecOutcome::Step);
                }
                let idx = idx as usize;
                if idx >= globals.len() {
                    return Ok(ExecOutcome::Step);
                }
                let table_id = match &globals[idx] {
                    Variant::Table(tid) => *tid,
                    _ => return Ok(ExecOutcome::Step),
                };
                runtime.table_set(table_id, key, value).map_err(|e| VmError::SyscallFailed { id: 0xFFFF, msg: format!("{e:#}") })?;
                Ok(ExecOutcome::Step)
            }

            PopLocalTable => {
                let local_off = self.read_u8()? as usize;
                let value = self.pop()?;
                let key = self.pop()?;
                let idx = self.stack_base + local_off;
                if idx >= self.stack.len() {
                    return Ok(ExecOutcome::Step);
                }
                let table_id = match &self.stack[idx] {
                    Variant::Table(tid) => *tid,
                    _ => return Ok(ExecOutcome::Step),
                };
                runtime.table_set(table_id, key, value).map_err(|e| VmError::SyscallFailed { id: 0xFFFF, msg: format!("{e:#}") })?;
                Ok(ExecOutcome::Step)
            }

            VmNeg => {
                let v = self.peek_mut()?;
                match v {
                    Variant::Int(i) => *i = i.wrapping_neg(),
                    Variant::Float(x) => *x = -*x,
                    _ => *v = Variant::Nil,
                }
                Ok(ExecOutcome::Step)
            }

            VmAdd => {
                let b = self.pop()?;
                let a = self.pop()?;
                let out = match (a, b) {
                    (Variant::Int(x), Variant::Int(y)) => Variant::Int(x.wrapping_add(y)),
                    (Variant::Int(x), Variant::Float(y)) => Variant::Float((x as f32) + y),
                    (Variant::Float(x), Variant::Int(y)) => Variant::Float(x + (y as f32)),
                    (Variant::Float(x), Variant::Float(y)) => Variant::Float(x + y),

                    (Variant::DynStr(x), Variant::DynStr(y)) => Variant::DynStr(format!("{x}{y}")),
                    _ => Variant::Nil,
                };
                self.push(out)?;
                Ok(ExecOutcome::Step)
            }

            VmSub | VmMul | VmDiv | VmMod => {
                let b = self.pop()?;
                let a = self.pop()?;
                let out = match (op, a, b) {
                    (VmSub, Variant::Int(x), Variant::Int(y)) => Variant::Int(x.wrapping_sub(y)),
                    (VmSub, Variant::Int(x), Variant::Float(y)) => Variant::Float((x as f32) - y),
                    (VmSub, Variant::Float(x), Variant::Int(y)) => Variant::Float(x - (y as f32)),
                    (VmSub, Variant::Float(x), Variant::Float(y)) => Variant::Float(x - y),

                    (VmMul, Variant::Int(x), Variant::Int(y)) => Variant::Int(x.wrapping_mul(y)),
                    (VmMul, Variant::Int(x), Variant::Float(y)) => Variant::Float((x as f32) * y),
                    (VmMul, Variant::Float(x), Variant::Int(y)) => Variant::Float(x * (y as f32)),
                    (VmMul, Variant::Float(x), Variant::Float(y)) => Variant::Float(x * y),

                    (VmDiv, Variant::Int(x), Variant::Int(y)) => Variant::Int(if y == 0 { 0 } else { x / y }),
                    (VmDiv, Variant::Int(x), Variant::Float(y)) => Variant::Float(if y == 0.0 { 0.0 } else { (x as f32) / y }),
                    (VmDiv, Variant::Float(x), Variant::Int(y)) => Variant::Float(if y == 0 { 0.0 } else { x / (y as f32) }),
                    (VmDiv, Variant::Float(x), Variant::Float(y)) => Variant::Float(if y == 0.0 { 0.0 } else { x / y }),

                    (VmMod, Variant::Int(x), Variant::Int(y)) => Variant::Int(if y == 0 { 0 } else { x % y }),
                    _ => Variant::Nil,
                };
                self.push(out)?;
                Ok(ExecOutcome::Step)
            }

            VmBitTest => {
                let b = self.pop()?;
                let a = self.pop()?;
                let out = match (a, b) {
                    (Variant::Int(x), Variant::Int(bit)) => {
                        let bit = bit.clamp(0, 31) as u32;
                        let ux = x as u32;
                        Variant::Bool(((1_u32 << bit) & ux) != 0)
                    }
                    _ => Variant::Nil,
                };
                self.push(out)?;
                Ok(ExecOutcome::Step)
            }

            VmAnd | VmOr => {
                let b = self.pop()?;
                let a = self.pop()?;
                let out = match op {
                    VmAnd => Variant::Bool(a.truthy() && b.truthy()),
                    VmOr => Variant::Bool(a.truthy() || b.truthy()),
                    _ => Variant::Nil,
                };
                self.push(out)?;
                Ok(ExecOutcome::Step)
            }

            VmSetE | VmSetNE | VmSetG | VmSetLE | VmSetL | VmSetGE => {
                let b = self.pop()?;
                let a = self.pop()?;
                let ord = self.cmp_variant(&a, &b);

                let out = match (op, ord) {
                    (VmSetE, Some(Ordering::Equal)) => true,
                    (VmSetE, None) => false,

                    (VmSetNE, Some(Ordering::Equal)) => false,
                    (VmSetNE, None) => true, // fallback: different types => not equal

                    (VmSetG, Some(Ordering::Greater)) => true,
                    (VmSetLE, Some(Ordering::Greater)) => false,

                    (VmSetL, Some(Ordering::Less)) => true,
                    (VmSetGE, Some(Ordering::Less)) => false,

                    (VmSetG, Some(_)) => false,
                    (VmSetLE, Some(_)) => true,

                    (VmSetL, Some(_)) => false,
                    (VmSetGE, Some(_)) => true,

                    // If not comparable, behave like false for ordering predicates.
                    _ => false,
                };

                self.push(Variant::Bool(out))?;
                Ok(ExecOutcome::Step)
            }
        }
    }

    fn ret_common(&mut self) -> std::result::Result<ExecOutcome, VmError> {
        let frame = self.call_stack.pop().ok_or(VmError::CallStackUnderflow)?;

        // Truncate operand stack to what it was before pushing args for this frame.
        self.stack.truncate(frame.prev_stack_len);
        self.stack_base = frame.prev_stack_base;

        if self.call_stack.is_empty() {
            return Ok(ExecOutcome::Halt);
        }
        self.pc = frame.return_pc;
        Ok(ExecOutcome::Step)
    }

    /// Execute until:
    /// - `max_steps` is reached, or
    /// - a `Halt` occurs.
    pub fn run<R: VmRuntime>(
        &mut self,
        runtime: &mut R,
        globals: &mut [Variant],
        max_steps: usize,
    ) -> Result<ExecOutcome> {
        for _ in 0..max_steps {
            match self.step(runtime, globals) {
                Ok(ExecOutcome::Step) => continue,
                Ok(ExecOutcome::Halt) => return Ok(ExecOutcome::Halt),
                Ok(ExecOutcome::Yield) => return Ok(ExecOutcome::Yield),
                Err(e) => return Err(anyhow!(e)),
            }
        }
        Ok(ExecOutcome::Yield)
    }
}
