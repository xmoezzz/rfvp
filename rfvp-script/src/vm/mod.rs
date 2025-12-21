use std::collections::HashMap;
use anyhow::{bail, Context, Result};

use crate::parser::HcbFile;

/// VM value type, kept close to the original 8-byte layout.
///
/// Type tags:
/// - 0: Nil
/// - 1: True
/// - 2: Int (Value=i32 as u32)
/// - 3: Float (Value=f32 bits)
/// - 4: ConstStr (Value=absolute offset into the script buffer; data is NUL-terminated)
/// - 5: DynStr (Value=string pool index)
/// - 6: Table  (Value=table pool index)
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Variant {
    pub ty: u8,
    pub local_count: u8,
    pub stack_base: u16,
    pub value: u32,
}

impl Variant {
    pub const NIL: Variant = Variant {
        ty: 0,
        local_count: 0,
        stack_base: 0,
        value: 0,
    };

    #[inline]
    pub fn is_nil(&self) -> bool {
        self.ty == 0
    }

    /// The original VM uses a "type != 0" truthiness convention (see vm_and/vm_or).
    #[inline]
    pub fn truthy(&self) -> bool {
        self.ty != 0
    }

    #[inline]
    pub fn int(v: i32) -> Variant {
        Variant {
            ty: 2,
            value: v as u32,
            ..Variant::NIL
        }
    }

    #[inline]
    pub fn float(v: f32) -> Variant {
        Variant {
            ty: 3,
            value: v.to_bits(),
            ..Variant::NIL
        }
    }

    #[inline]
    pub fn const_str(off: u32) -> Variant {
        Variant {
            ty: 4,
            value: off,
            ..Variant::NIL
        }
    }

    #[inline]
    pub fn dyn_str(idx: u32) -> Variant {
        Variant {
            ty: 5,
            value: idx,
            ..Variant::NIL
        }
    }

    #[inline]
    pub fn table(idx: u32) -> Variant {
        Variant {
            ty: 6,
            value: idx,
            ..Variant::NIL
        }
    }
}

#[derive(Debug, Clone)]
pub enum ValueView<'v> {
    Nil,
    True,
    Int(i32),
    Float(f32),
    /// Raw bytes (until NUL); encoding is game-dependent.
    Str(&'v [u8]),
    /// Dynamic string bytes (until NUL).
    DynStr(&'v [u8]),
    Table(&'v HashMap<u32, Variant>),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ControlFlow {
    Continue,
    /// Yield control back to the external executor.
    Yield,
    /// Halt the current thread.
    Halt,
}

#[derive(Debug, Clone)]
pub struct SyscallResult {
    pub ret: Variant,
    pub control: ControlFlow,
}

pub trait SyscallHost {
    type Handle: Copy + Eq;

    /// Resolve an imported syscall (name + arg count) into a host handle.
    fn resolve(&mut self, name: &[u8], arg_count: u8) -> Result<Self::Handle>;

    /// Call a previously resolved syscall.
    fn call(
        &mut self,
        handle: Self::Handle,
        args: &[Variant],
        ctx: &mut VmContext<'_>,
    ) -> Result<SyscallResult>;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    Continue,
    Yield,
    Halt,
}

#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub outcome: StepOutcome,
    /// How many bytecode instructions were executed in this run.
    pub steps: usize,
}

#[derive(Debug, Copy, Clone)]
struct ImportResolved<H: Copy + Eq> {
    arg_count: u8,
    handle: H,
}

#[derive(Debug)]
struct StringPool {
    data: Vec<Option<Vec<u8>>>,
    refcnt: Vec<u32>,
}

impl StringPool {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            refcnt: Vec::new(),
        }
    }

    fn alloc(&mut self, s: Vec<u8>) -> u32 {
        let idx = self.data.len() as u32;
        self.data.push(Some(s));
        self.refcnt.push(1);
        idx
    }

    fn get(&self, idx: u32) -> Option<&[u8]> {
        self.data.get(idx as usize)?.as_deref()
    }

    fn inc(&mut self, idx: u32) {
        if let Some(c) = self.refcnt.get_mut(idx as usize) {
            *c = c.saturating_add(1);
        }
    }

    fn dec(&mut self, idx: u32) {
        let Some(c) = self.refcnt.get_mut(idx as usize) else {
            return;
        };
        if *c > 1 {
            *c -= 1;
            return;
        }
        *c = 0;
        if let Some(slot) = self.data.get_mut(idx as usize) {
            *slot = None;
        }
    }
}

#[derive(Debug)]
struct TablePool {
    data: Vec<HashMap<u32, Variant>>,
    refcnt: Vec<u32>,
}

impl TablePool {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            refcnt: Vec::new(),
        }
    }

    fn ensure_slot(&mut self, idx: u32) {
        let need = idx as usize + 1;
        if self.data.len() < need {
            self.data.resize_with(need, HashMap::new);
            self.refcnt.resize(need, 0);
        }
    }

    fn alloc(&mut self) -> u32 {
        let idx = self.data.len() as u32;
        self.data.push(HashMap::new());
        self.refcnt.push(1);
        idx
    }

    /// Get a table slot. If the index is out of range, this will *initialize* an empty table slot.
    ///
    /// This matches the original engine's practical behavior: tables are stored in a scene-owned
    /// table array, and "freed" tables are cleared and later reused rather than becoming invalid pointers.
    fn get_or_init(&mut self, idx: u32) -> &HashMap<u32, Variant> {
        self.ensure_slot(idx);
        &self.data[idx as usize]
    }

    fn get_or_init_mut(&mut self, idx: u32) -> &mut HashMap<u32, Variant> {
        self.ensure_slot(idx);
        &mut self.data[idx as usize]
    }

    fn inc(&mut self, idx: u32) {
        self.ensure_slot(idx);
        self.refcnt[idx as usize] = self.refcnt[idx as usize].saturating_add(1);
    }

    fn dec(&mut self, idx: u32, strings: &mut StringPool) {
        self.ensure_slot(idx);
        let c = &mut self.refcnt[idx as usize];
        if *c == 0 {
            // Already cleared.
            return;
        }
        if *c > 1 {
            *c -= 1;
            return;
        }
        // last reference
        *c = 0;

        // Drain values so we can drop nested refs without holding a borrow into the map.
        let values: Vec<Variant> = self.data[idx as usize].drain().map(|(_, v)| v).collect();
        for mut v in values {
            clear_var(&mut v, strings, self);
        }
        // Now table slot exists and is empty.
    }
}



/// View/allocate helpers exposed to `SyscallHost`.
pub struct VmContext<'a> {
    bytes: &'a [u8],
    strings: &'a mut StringPool,
    tables: &'a mut TablePool,
}

impl<'a> VmContext<'a> {
    pub fn view<'v>(&'v mut self, v: &Variant) -> ValueView<'v> {
        match v.ty {
            0 => ValueView::Nil,
            1 => ValueView::True,
            2 => ValueView::Int(v.value as i32),
            3 => ValueView::Float(f32::from_bits(v.value)),
            4 => ValueView::Str(read_c_string(self.bytes, v.value as usize)),
            5 => ValueView::DynStr(self.strings.get(v.value).unwrap_or(b"")),
            6 => ValueView::Table(self.tables.get_or_init(v.value)),
            _ => ValueView::Nil,
        }
    }

    /// Allocate a dynamic string in the VM pool and return a `Variant` pointing to it.
    ///
    /// The pool stores bytes as-is. If the data is not NUL-terminated, it will be appended.
    pub fn alloc_dyn_str(&mut self, mut bytes: Vec<u8>) -> Variant {
        if !bytes.ends_with(&[0]) {
            bytes.push(0);
        }
        let idx = self.strings.alloc(bytes);
        Variant::dyn_str(idx)
    }

    /// Allocate a new empty table and return a `Variant` pointing to it.
    pub fn alloc_table(&mut self) -> Variant {
        let idx = self.tables.alloc();
        Variant::table(idx)
    }
}

fn read_c_string(bytes: &[u8], off: usize) -> &[u8] {
    if off >= bytes.len() {
        return b"";
    }
    let tail = &bytes[off..];
    match memchr::memchr(0, tail) {
        Some(n) => &tail[..n],
        None => tail,
    }
}

fn clear_var(v: &mut Variant, strings: &mut StringPool, tables: &mut TablePool) {
    match v.ty {
        5 => strings.dec(v.value),
        6 => tables.dec(v.value, strings),
        _ => {}
    }
    *v = Variant::NIL;
}

fn inc_ref(v: &Variant, strings: &mut StringPool, tables: &mut TablePool) {
    match v.ty {
        5 => strings.inc(v.value),
        6 => tables.inc(v.value),
        _ => {}
    }
}

/// IMPORTANT: `Vm` is a runtime object with reference-counted pools; it must not be `Clone`.
#[derive(Debug)]
pub struct Vm {
    file: HcbFile,
    imports: Vec<ImportResolved<u32>>,
    globals: Vec<Variant>,

    // runtime stores
    strings: StringPool,
    tables: TablePool,

    // thread context (single-thread VM in this crate; scheduler lives outside)
    pc: u32,
    stack_top: u32,  // next free
    stack_base: u32, // current frame base (arguments/local variable base)
    stack: [Variant; 256],
    return_value: Variant,

    pub should_break: bool,
    pub halt: bool,
}

impl Vm {
    pub fn new<H: SyscallHost<Handle = u32>>(file: HcbFile, host: &mut H) -> Result<Self> {
        let mut imports = Vec::with_capacity(file.imports.len());
        for imp in &file.imports {
            let handle = host
                .resolve(&imp.name, imp.arg_count)
                .with_context(|| {
                    format!(
                        "resolve syscall: name={:?} argc={}",
                        imp.name, imp.arg_count
                    )
                })?;
            imports.push(ImportResolved {
                arg_count: imp.arg_count,
                handle,
            });
        }

        let globals = vec![Variant::NIL; file.total_global_count()];

        // Main thread bootstrapping:
        // Provide a synthetic "call frame source slot" at stack_top=1 so init_stackp can relocate it.
        let mut stack = [Variant::NIL; 256];
        let stack_top = 1u32;
        stack[1] = Variant {
            ty: 0,          // will be overwritten by init_stackp (args_cnt)
            local_count: 0, // will be overwritten by init_stackp (local_cnt)
            stack_base: 0,  // prev base
            value: 0,       // return PC = 0 => HALT on ret
        };

        Ok(Self {
            pc: file.entry_point,
            stack_top,
            stack_base: 1,
            stack,
            return_value: Variant::NIL,
            should_break: false,
            halt: false,
            file,
            imports,
            globals,
            strings: StringPool::new(),
            tables: TablePool::new(),
        })
    }

    pub fn file(&self) -> &HcbFile {
        &self.file
    }

    pub fn globals(&self) -> &[Variant] {
        &self.globals
    }

    pub fn globals_mut(&mut self) -> &mut [Variant] {
        &mut self.globals
    }

    pub fn pc(&self) -> u32 {
        self.pc
    }

    pub fn run_for<H: SyscallHost<Handle = u32>>(
        &mut self,
        host: &mut H,
        budget: usize,
    ) -> Result<RunOutcome> {
        let mut steps = 0usize;
        while steps < budget && !self.halt {
            match self.step(host)? {
                StepOutcome::Continue => {
                    steps += 1;
                }
                StepOutcome::Yield => {
                    steps += 1;
                    return Ok(RunOutcome {
                        outcome: StepOutcome::Yield,
                        steps,
                    });
                }
                StepOutcome::Halt => {
                    steps += 1;
                    return Ok(RunOutcome {
                        outcome: StepOutcome::Halt,
                        steps,
                    });
                }
            }
        }
        Ok(RunOutcome {
            outcome: if self.halt {
                StepOutcome::Halt
            } else {
                StepOutcome::Continue
            },
            steps,
        })
    }

    pub fn step<H: SyscallHost<Handle = u32>>(&mut self, host: &mut H) -> Result<StepOutcome> {
        if self.pc as usize >= self.file.bytes.len() {
            bail!("pc out of range: pc={} len={}", self.pc, self.file.bytes.len());
        }

        let op = self.file.bytes[self.pc as usize];
        self.pc = self.pc.wrapping_add(1);

        match op {
            0 => Ok(StepOutcome::Continue), // nop

            1 => self.op_init_stack(),

            2 => self.op_call(),

            3 => self.op_syscall(host),

            4 => self.op_ret(),

            5 => self.op_retv(),

            6 => self.op_jmp(),

            7 => self.op_jz(),

            8 => {
                self.push(Variant::NIL)?;
                Ok(StepOutcome::Continue)
            }

            9 => {
                self.push(Variant { ty: 1, ..Variant::NIL })?;
                Ok(StepOutcome::Continue)
            }

            10 => self.op_push_i32(),

            11 => self.op_push_i16(),

            12 => self.op_push_i8(),

            13 => self.op_push_f32(),

            14 => self.op_push_str(),

            15 => self.op_push_global(),

            16 => self.op_push_stack(),

            17 => self.op_push_global_table(),

            18 => self.op_push_local_table(),

            19 => self.op_push_top(),

            20 => self.op_push_ret_value(),

            21 => self.op_pop_global(),

            22 => self.op_local_copy(),

            23 => self.op_pop_global_table(),

            24 => self.op_pop_local_table(),

            25 => self.op_vm_neg(),

            26 => self.op_vm_add(),

            27 => self.op_vm_sub(),

            28 => self.op_vm_mul(),

            29 => self.op_vm_div(),

            30 => self.op_vm_mod(),

            31 => self.op_vm_bittest(),

            32 => self.op_vm_and(),

            33 => self.op_vm_or(),

            34 => self.op_vm_sete(),

            35 => self.op_vm_setne(),

            36 => self.op_vm_setg(),

            37 => self.op_vm_setle(),

            38 => self.op_vm_setl(),

            39 => self.op_vm_setge(),

            _ => bail!("unknown opcode {} at pc={}", op, self.pc.wrapping_sub(1)),
        }
    }

    // -------------------------
    // Low-level buffer readers
    // -------------------------

    fn read_u8(&mut self) -> Result<u8> {
        let pc = self.pc as usize;
        if pc + 1 > self.file.bytes.len() {
            bail!("unexpected EOF while reading u8 at pc={}", pc);
        }
        let v = self.file.bytes[pc];
        self.pc += 1;
        Ok(v)
    }

    fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    fn read_u16(&mut self) -> Result<u16> {
        let pc = self.pc as usize;
        if pc + 2 > self.file.bytes.len() {
            bail!("unexpected EOF while reading u16 at pc={}", pc);
        }
        let v = u16::from_le_bytes([self.file.bytes[pc], self.file.bytes[pc + 1]]);
        self.pc += 2;
        Ok(v)
    }

    fn read_u32(&mut self) -> Result<u32> {
        let pc = self.pc as usize;
        if pc + 4 > self.file.bytes.len() {
            bail!("unexpected EOF while reading u32 at pc={}", pc);
        }
        let v = u32::from_le_bytes([
            self.file.bytes[pc],
            self.file.bytes[pc + 1],
            self.file.bytes[pc + 2],
            self.file.bytes[pc + 3],
        ]);
        self.pc += 4;
        Ok(v)
    }

    // -------------------------
    // Stack helpers
    // -------------------------

    fn top_idx(&self) -> Result<usize> {
        if self.stack_top == 0 {
            bail!("stack underflow");
        }
        Ok((self.stack_top - 1) as usize)
    }

    fn push(&mut self, v: Variant) -> Result<()> {
        let sp = self.stack_top as usize;
        if sp >= self.stack.len() {
            bail!("stack overflow");
        }
        self.stack[sp] = v;
        self.stack_top += 1;
        Ok(())
    }

    /// Pop but *transfer ownership* to the caller (no decref).
    fn pop_take(&mut self) -> Result<Variant> {
        if self.stack_top == 0 {
            bail!("stack underflow");
        }
        self.stack_top -= 1;
        let idx = self.stack_top as usize;
        let v = self.stack[idx];
        self.stack[idx] = Variant::NIL;
        Ok(v)
    }

    /// Consume RHS and return (lhs_slot_index, lhs_value, rhs_value).
    ///
    /// Semantics: equivalent to the original VM pattern:
    ///   --StackTop; lhs = stack[StackTop-1], rhs = stack[StackTop]
    /// with RHS popped from the stack.
    fn bin_take(&mut self) -> Result<(usize, Variant, Variant)> {
        if self.stack_top < 2 {
            bail!("stack underflow");
        }
        self.stack_top -= 1;
        let rhs_idx = self.stack_top as usize;
        let lhs_idx = rhs_idx - 1;

        let lhs = self.stack[lhs_idx];
        let rhs = self.stack[rhs_idx];

        // Detach both slots from the stack to avoid borrow conflicts while touching pools.
        self.stack[lhs_idx] = Variant::NIL;
        self.stack[rhs_idx] = Variant::NIL;

        Ok((lhs_idx, lhs, rhs))
    }

    fn pop_args(&mut self, argc: u8) -> Result<Vec<Variant>> {
        let argc = argc as u32;
        if self.stack_top < argc {
            bail!(
                "stack underflow in syscall: argc={} stack_top={}",
                argc,
                self.stack_top
            );
        }
        let mut args = Vec::with_capacity(argc as usize);
        for _ in 0..argc {
            args.push(self.pop_take()?);
        }
        args.reverse();
        Ok(args)
    }

    // -------------------------
    // Frame helpers
    // -------------------------

    fn init_stackp(&mut self, args_cnt: u8, local_cnt: i8) -> Result<()> {
        let local_cnt: u8 = local_cnt
            .try_into()
            .context("negative local_cnt is not supported")?;

        // Frame source slot is at current stack_top (call stores it without advancing stack_top).
        let src_frame = self.stack_top as usize;
        if src_frame >= self.stack.len() {
            bail!("stack overflow (frame)");
        }

        let args_cnt_u32 = args_cnt as u32;
        if self.stack_top < args_cnt_u32 + 1 {
            bail!(
                "stack underflow in init_stackp: stack_top={} args_cnt={}",
                self.stack_top,
                args_cnt
            );
        }

        // Destination is one slot below the argument block.
        let dst_frame = (self.stack_top - args_cnt_u32 - 1) as usize;

        // Move frame record down below arguments.
        let mut frame = self.stack[src_frame];
        self.stack[src_frame] = Variant::NIL;

        // Store args/local metadata in the frame record.
        frame.ty = args_cnt;
        frame.local_count = local_cnt;

        self.stack[dst_frame] = frame;

        // New frame base points right above the frame record.
        self.stack_base = (dst_frame + 1) as u32;

        // After relocation, StackTop points right above arguments.
        self.stack_top = self.stack_base + args_cnt_u32;

        // Initialize locals above arguments.
        let need = self.stack_top as usize + local_cnt as usize;
        if need > self.stack.len() {
            bail!(
                "stack overflow allocating locals: need={} cap={}",
                need,
                self.stack.len()
            );
        }
        for i in 0..local_cnt as usize {
            self.stack[self.stack_top as usize + i] = Variant::NIL;
        }
        self.stack_top += local_cnt as u32;

        Ok(())
    }

    fn ret_p(&mut self) -> Result<()> {
        if self.stack_base == 0 {
            self.halt = true;
            return Ok(());
        }
        let frame_idx = (self.stack_base - 1) as usize;
        let frame = self.stack[frame_idx];

        // Drop locals/args/temps above the frame record.
        // IMPORTANT: do NOT call clear_var() on the frame record itself; frame.ty/local_count store
        // counts and can be 5/6, which would corrupt refcounts if treated as a value.
        while self.stack_top as usize > frame_idx + 1 {
            self.stack_top -= 1;
            let idx = self.stack_top as usize;
            let mut v = self.stack[idx];
            clear_var(&mut v, &mut self.strings, &mut self.tables);
            self.stack[idx] = Variant::NIL;
        }

        // Pop the frame record (without clear_var).
        self.stack[frame_idx] = Variant::NIL;
        self.stack_top = frame_idx as u32;

        self.stack_base = frame.stack_base as u32;
        self.pc = frame.value;

        if self.pc == 0 {
            self.halt = true;
        }

        Ok(())
    }

    // -------------------------
    // Opcode handlers
    // -------------------------

    fn op_init_stack(&mut self) -> Result<StepOutcome> {
        let args_cnt = self.read_u8()?;
        let local_cnt = self.read_i8()?;
        self.init_stackp(args_cnt, local_cnt)?;
        Ok(StepOutcome::Continue)
    }

    fn op_call(&mut self) -> Result<StepOutcome> {
        let target = self.read_u32()?;
        let ret_pc = self.pc;

        let sp = self.stack_top as usize;
        if sp >= self.stack.len() {
            bail!("stack overflow (call frame)");
        }
        self.stack[sp] = Variant {
            ty: 0,
            local_count: 0,
            stack_base: self.stack_base as u16,
            value: ret_pc,
        };

        self.pc = target;
        Ok(StepOutcome::Continue)
    }

    fn op_syscall<H: SyscallHost<Handle = u32>>(&mut self, host: &mut H) -> Result<StepOutcome> {
        let import_id = self.read_u16()? as usize;
        let Some(imp) = self.imports.get(import_id).copied() else {
            bail!("unknown import id={}", import_id);
        };

        let args = self.pop_args(imp.arg_count)?;

        let mut ctx = VmContext {
            bytes: &self.file.bytes,
            strings: &mut self.strings,
            tables: &mut self.tables,
        };

        let res = host
            .call(imp.handle, &args, &mut ctx)
            .with_context(|| format!("syscall failed: import_id={}", import_id))?;

        for mut v in args.into_iter() {
            clear_var(&mut v, &mut self.strings, &mut self.tables);
        }

        clear_var(&mut self.return_value, &mut self.strings, &mut self.tables);
        self.return_value = res.ret;

        Ok(match res.control {
            ControlFlow::Continue => StepOutcome::Continue,
            ControlFlow::Yield => StepOutcome::Yield,
            ControlFlow::Halt => {
                self.halt = true;
                StepOutcome::Halt
            }
        })
    }

    fn op_ret(&mut self) -> Result<StepOutcome> {
        clear_var(&mut self.return_value, &mut self.strings, &mut self.tables);
        self.return_value = Variant::NIL;
        self.ret_p()?;
        Ok(if self.halt {
            StepOutcome::Halt
        } else {
            StepOutcome::Continue
        })
    }

    fn op_retv(&mut self) -> Result<StepOutcome> {
        clear_var(&mut self.return_value, &mut self.strings, &mut self.tables);
        let v = self.pop_take()?; // transfer
        self.return_value = v;
        self.ret_p()?;
        Ok(if self.halt {
            StepOutcome::Halt
        } else {
            StepOutcome::Continue
        })
    }

    fn op_jmp(&mut self) -> Result<StepOutcome> {
        let target = self.read_u32()?;
        self.pc = target;
        Ok(StepOutcome::Continue)
    }

    fn op_jz(&mut self) -> Result<StepOutcome> {
        let mut cond = self.pop_take()?;
        let target = self.read_u32()?;
        if cond.ty == 0 {
            self.pc = target;
        }
        clear_var(&mut cond, &mut self.strings, &mut self.tables);
        Ok(StepOutcome::Continue)
    }

    fn op_push_i32(&mut self) -> Result<StepOutcome> {
        let v = self.read_u32()? as i32;
        self.push(Variant::int(v))?;
        Ok(StepOutcome::Continue)
    }

    fn op_push_i16(&mut self) -> Result<StepOutcome> {
        let v = self.read_u16()? as i16 as i32;
        self.push(Variant::int(v))?;
        Ok(StepOutcome::Continue)
    }

    fn op_push_i8(&mut self) -> Result<StepOutcome> {
        let v = self.read_u8()? as i8 as i32;
        self.push(Variant::int(v))?;
        Ok(StepOutcome::Continue)
    }

    fn op_push_f32(&mut self) -> Result<StepOutcome> {
        let bits = self.read_u32()?;
        self.push(Variant {
            ty: 3,
            value: bits,
            ..Variant::NIL
        })?;
        Ok(StepOutcome::Continue)
    }

    fn op_push_str(&mut self) -> Result<StepOutcome> {
        let len = self.read_u8()? as usize;
        let off = self.pc as usize;
        self.pc = self.pc.wrapping_add(len as u32);
        self.push(Variant::const_str(off as u32))?;
        Ok(StepOutcome::Continue)
    }

    fn op_push_global(&mut self) -> Result<StepOutcome> {
        let idx = self.read_u16()? as usize;
        let Some(v) = self.globals.get(idx).copied() else {
            bail!("global index out of range: {}", idx);
        };
        inc_ref(&v, &mut self.strings, &mut self.tables);
        self.push(v)?;
        Ok(StepOutcome::Continue)
    }

    fn op_push_stack(&mut self) -> Result<StepOutcome> {
        let off = self.read_u8()? as u32;
        let idx = self.stack_base + off;
        if idx as usize >= self.stack.len() {
            bail!(
                "stack local index out of range: base={} off={}",
                self.stack_base,
                off
            );
        }
        let v = self.stack[idx as usize];
        inc_ref(&v, &mut self.strings, &mut self.tables);
        self.push(v)?;
        Ok(StepOutcome::Continue)
    }

    fn op_push_global_table(&mut self) -> Result<StepOutcome> {
        let global_idx = self.read_u16()? as usize;
        let top_idx = self.top_idx()?;
        let key = self.stack[top_idx];

        let Some(gv) = self.globals.get(global_idx).copied() else {
            bail!("global index out of range: {}", global_idx);
        };

        let found = if gv.ty == 6 && key.ty == 2 {
            let k = key.value as u32;
            let map = self.tables.get_or_init(gv.value);
            map.get(&k).copied()
        } else {
            None
        };

        // Always consume the key on stack (original behavior: overwrite top with found or NIL).
        let mut old_key = key;
        self.stack[top_idx] = Variant::NIL;
        clear_var(&mut old_key, &mut self.strings, &mut self.tables);

        if let Some(v) = found {
            inc_ref(&v, &mut self.strings, &mut self.tables);
            self.stack[top_idx] = v;
        } else {
            self.stack[top_idx] = Variant::NIL;
        }

        Ok(StepOutcome::Continue)
    }

    fn op_push_local_table(&mut self) -> Result<StepOutcome> {
        let local_off = self.read_u8()? as u32;
        let local_idx = self.stack_base + local_off;
        if local_idx as usize >= self.stack.len() {
            bail!("local index out of range for table lookup");
        }

        let top_idx = self.top_idx()?;
        let key = self.stack[top_idx];
        let local_v = self.stack[local_idx as usize];

        let found = if local_v.ty == 6 && key.ty == 2 {
            let k = key.value as u32;
            let map = self.tables.get_or_init(local_v.value);
            map.get(&k).copied()
        } else {
            None
        };

        // Consume key on top.
        let mut old_key = key;
        self.stack[top_idx] = Variant::NIL;
        clear_var(&mut old_key, &mut self.strings, &mut self.tables);

        if let Some(v) = found {
            inc_ref(&v, &mut self.strings, &mut self.tables);
            self.stack[top_idx] = v;
        } else {
            self.stack[top_idx] = Variant::NIL;
        }

        Ok(StepOutcome::Continue)
    }

    fn op_push_top(&mut self) -> Result<StepOutcome> {
        let idx = self.top_idx()?;
        let v = self.stack[idx];
        inc_ref(&v, &mut self.strings, &mut self.tables);
        self.push(v)?;
        Ok(StepOutcome::Continue)
    }

    fn op_push_ret_value(&mut self) -> Result<StepOutcome> {
        let v = self.return_value;
        self.return_value = Variant::NIL;
        inc_ref(&v, &mut self.strings, &mut self.tables);
        self.push(v)?;
        Ok(StepOutcome::Continue)
    }

    fn op_pop_global(&mut self) -> Result<StepOutcome> {
        let idx = self.read_u16()? as usize;
        if idx >= self.globals.len() {
            bail!("global index out of range: {}", idx);
        }
        let v = self.pop_take()?; // transfer
        let mut old = self.globals[idx];
        clear_var(&mut old, &mut self.strings, &mut self.tables);
        self.globals[idx] = v;
        Ok(StepOutcome::Continue)
    }

    fn op_local_copy(&mut self) -> Result<StepOutcome> {
        let off = self.read_u8()? as u32;
        let idx = self.stack_base + off;
        if idx as usize >= self.stack.len() {
            bail!("local index out of range");
        }
        let v = self.pop_take()?; // transfer
        let mut old = self.stack[idx as usize];
        clear_var(&mut old, &mut self.strings, &mut self.tables);
        self.stack[idx as usize] = v;
        Ok(StepOutcome::Continue)
    }

    fn op_pop_global_table(&mut self) -> Result<StepOutcome> {
        let global_idx = self.read_u16()? as usize;
        if global_idx >= self.globals.len() {
            bail!("global index out of range");
        }

        let value_v = self.pop_take()?;
        let mut key_v = self.pop_take()?;

        if key_v.ty != 2 {
            // If key is not int, clear the target global and discard both operands.
            let mut g = self.globals[global_idx];
            clear_var(&mut g, &mut self.strings, &mut self.tables);
            self.globals[global_idx] = Variant::NIL;

            let mut vv = value_v;
            clear_var(&mut vv, &mut self.strings, &mut self.tables);
            clear_var(&mut key_v, &mut self.strings, &mut self.tables);
            return Ok(StepOutcome::Continue);
        }

        if self.globals[global_idx].ty != 6 {
            let mut g = self.globals[global_idx];
            clear_var(&mut g, &mut self.strings, &mut self.tables);
            let tidx = self.tables.alloc();
            self.globals[global_idx] = Variant::table(tidx);
        }

        let tidx = self.globals[global_idx].value;
        let k = key_v.value as u32;

        // Remove old entry (drop mutable borrow before touching pools).
        let old = {
            let map = self.tables.get_or_init_mut(tidx);
            map.remove(&k)
        };

        if let Some(mut o) = old {
            clear_var(&mut o, &mut self.strings, &mut self.tables);
        }

        {
            let map = self.tables.get_or_init_mut(tidx);
            map.insert(k, value_v);
        }

        clear_var(&mut key_v, &mut self.strings, &mut self.tables);
        Ok(StepOutcome::Continue)
    }

    fn op_pop_local_table(&mut self) -> Result<StepOutcome> {
        let local_off = self.read_u8()? as u32;
        let local_idx = self.stack_base + local_off;
        if local_idx as usize >= self.stack.len() {
            bail!("local index out of range");
        }

        let value_v = self.pop_take()?;
        let mut key_v = self.pop_take()?;

        if key_v.ty != 2 {
            // If key is not int, clear the local and discard both operands.
            let mut l = self.stack[local_idx as usize];
            clear_var(&mut l, &mut self.strings, &mut self.tables);
            self.stack[local_idx as usize] = Variant::NIL;

            let mut vv = value_v;
            clear_var(&mut vv, &mut self.strings, &mut self.tables);
            clear_var(&mut key_v, &mut self.strings, &mut self.tables);
            return Ok(StepOutcome::Continue);
        }

        // Ensure local is a table.
        if self.stack[local_idx as usize].ty != 6 {
            let mut old = self.stack[local_idx as usize];
            clear_var(&mut old, &mut self.strings, &mut self.tables);
            let tidx = self.tables.alloc();
            self.stack[local_idx as usize] = Variant::table(tidx);
        }

        let tidx = self.stack[local_idx as usize].value;
        let k = key_v.value as u32;

        // Remove old entry (drop borrow before touching pools).
        let old = {
            let map = self.tables.get_or_init_mut(tidx);
            map.remove(&k)
        };

        if let Some(mut o) = old {
            clear_var(&mut o, &mut self.strings, &mut self.tables);
        }

        {
            let map = self.tables.get_or_init_mut(tidx);
            map.insert(k, value_v);
        }

        clear_var(&mut key_v, &mut self.strings, &mut self.tables);
        Ok(StepOutcome::Continue)
    }

    fn op_vm_neg(&mut self) -> Result<StepOutcome> {
        let idx = self.top_idx()?;
        let mut v = self.stack[idx];

        match v.ty {
            2 => {
                v.value = (-(v.value as i32)) as u32;
                self.stack[idx] = v;
            }
            3 => {
                let f = -f32::from_bits(v.value);
                v.value = f.to_bits();
                self.stack[idx] = v;
            }
            _ => {
                // Clear invalid operand.
                self.stack[idx] = Variant::NIL;
                clear_var(&mut v, &mut self.strings, &mut self.tables);
            }
        }
        Ok(StepOutcome::Continue)
    }

    fn op_vm_add(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;

        let out = match (lhs.ty, rhs.ty) {
            (2, 2) => {
                lhs.value = (lhs.value as i32).wrapping_add(rhs.value as i32) as u32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                lhs
            }
            (2, 3) => {
                let f = f32::from_bits(rhs.value) + (lhs.value as i32) as f32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (3, 2) => {
                let f = f32::from_bits(lhs.value) + (rhs.value as i32) as f32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (3, 3) => {
                let f = f32::from_bits(lhs.value) + f32::from_bits(rhs.value);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (4, 4) => {
                let a = read_c_string(&self.file.bytes, lhs.value as usize);
                let b = read_c_string(&self.file.bytes, rhs.value as usize);
                let mut out = Vec::with_capacity(a.len() + b.len() + 1);
                out.extend_from_slice(a);
                out.extend_from_slice(b);
                out.push(0);
                let idx = self.strings.alloc(out);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::dyn_str(idx)
            }
            (4, 5) => {
                let a = read_c_string(&self.file.bytes, lhs.value as usize);
                let b_len = self.strings.get(rhs.value).map(|s| s.len()).unwrap_or(0);
                let mut out = Vec::with_capacity(a.len() + b_len + 1);
                out.extend_from_slice(a);
                if let Some(b) = self.strings.get(rhs.value) {
                    out.extend_from_slice(b);
                }
                if !out.ends_with(&[0]) {
                    out.push(0);
                }
                let idx = self.strings.alloc(out);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::dyn_str(idx)
            }
            (5, 4) => {
                let a_len = self.strings.get(lhs.value).map(|s| s.len()).unwrap_or(0);
                let b = read_c_string(&self.file.bytes, rhs.value as usize);
                let mut out = Vec::with_capacity(a_len + b.len() + 1);
                if let Some(a) = self.strings.get(lhs.value) {
                    out.extend_from_slice(a);
                }
                out.extend_from_slice(b);
                if !out.ends_with(&[0]) {
                    out.push(0);
                }
                let idx = self.strings.alloc(out);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                clear_var(&mut lhs, &mut self.strings, &mut self.tables);
                Variant::dyn_str(idx)
            }
            (5, 5) => {
                let a_len = self.strings.get(lhs.value).map(|s| s.len()).unwrap_or(0);
                let b_len = self.strings.get(rhs.value).map(|s| s.len()).unwrap_or(0);
                let mut out = Vec::with_capacity(a_len + b_len + 1);
                if let Some(a) = self.strings.get(lhs.value) {
                    out.extend_from_slice(a);
                }
                if let Some(b) = self.strings.get(rhs.value) {
                    out.extend_from_slice(b);
                }
                if !out.ends_with(&[0]) {
                    out.push(0);
                }
                let idx = self.strings.alloc(out);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                clear_var(&mut lhs, &mut self.strings, &mut self.tables);
                Variant::dyn_str(idx)
            }
            _ => {
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                clear_var(&mut lhs, &mut self.strings, &mut self.tables);
                Variant::NIL
            }
        };

        self.stack[lhs_idx] = out;
        Ok(StepOutcome::Continue)
    }

    fn op_vm_sub(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;

        let out = match (lhs.ty, rhs.ty) {
            (2, 2) => {
                lhs.value = (lhs.value as i32).wrapping_sub(rhs.value as i32) as u32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                lhs
            }
            (2, 3) => {
                let f = (lhs.value as i32) as f32 - f32::from_bits(rhs.value);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (3, 2) => {
                let f = f32::from_bits(lhs.value) - (rhs.value as i32) as f32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (3, 3) => {
                let f = f32::from_bits(lhs.value) - f32::from_bits(rhs.value);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            _ => {
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                clear_var(&mut lhs, &mut self.strings, &mut self.tables);
                Variant::NIL
            }
        };

        self.stack[lhs_idx] = out;
        Ok(StepOutcome::Continue)
    }

    fn op_vm_mul(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;

        let out = match (lhs.ty, rhs.ty) {
            (2, 2) => {
                lhs.value = (lhs.value as i32).wrapping_mul(rhs.value as i32) as u32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                lhs
            }
            (2, 3) => {
                let f = (lhs.value as i32) as f32 * f32::from_bits(rhs.value);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (3, 2) => {
                let f = f32::from_bits(lhs.value) * (rhs.value as i32) as f32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (3, 3) => {
                let f = f32::from_bits(lhs.value) * f32::from_bits(rhs.value);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            _ => {
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                clear_var(&mut lhs, &mut self.strings, &mut self.tables);
                Variant::NIL
            }
        };

        self.stack[lhs_idx] = out;
        Ok(StepOutcome::Continue)
    }

    fn op_vm_div(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;

        let out = match (lhs.ty, rhs.ty) {
            (2, 2) => {
                lhs.value = ((lhs.value as i32) / (rhs.value as i32)) as u32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                lhs
            }
            (2, 3) => {
                let f = (lhs.value as i32) as f32 / f32::from_bits(rhs.value);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (3, 2) => {
                let f = f32::from_bits(lhs.value) / (rhs.value as i32) as f32;
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            (3, 3) => {
                let f = f32::from_bits(lhs.value) / f32::from_bits(rhs.value);
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                Variant::float(f)
            }
            _ => {
                clear_var(&mut rhs, &mut self.strings, &mut self.tables);
                clear_var(&mut lhs, &mut self.strings, &mut self.tables);
                Variant::NIL
            }
        };

        self.stack[lhs_idx] = out;
        Ok(StepOutcome::Continue)
    }

    fn op_vm_mod(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let out = if lhs.ty == 2 && rhs.ty == 2 {
            lhs.value = ((lhs.value as i32) % (rhs.value as i32)) as u32;
            clear_var(&mut rhs, &mut self.strings, &mut self.tables);
            lhs
        } else {
            clear_var(&mut rhs, &mut self.strings, &mut self.tables);
            clear_var(&mut lhs, &mut self.strings, &mut self.tables);
            Variant::NIL
        };
        self.stack[lhs_idx] = out;
        Ok(StepOutcome::Continue)
    }

    fn op_vm_bittest(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let out = if lhs.ty == 2 && rhs.ty == 2 {
            let bit = (1u32 << (rhs.value & 31)) & lhs.value;
            clear_var(&mut rhs, &mut self.strings, &mut self.tables);
            Variant {
                ty: if bit != 0 { 1 } else { 0 },
                ..Variant::NIL
            }
        } else {
            clear_var(&mut rhs, &mut self.strings, &mut self.tables);
            clear_var(&mut lhs, &mut self.strings, &mut self.tables);
            Variant::NIL
        };
        self.stack[lhs_idx] = out;
        Ok(StepOutcome::Continue)
    }

    fn op_vm_and(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let res = lhs.truthy() && rhs.truthy();
        clear_var(&mut rhs, &mut self.strings, &mut self.tables);
        clear_var(&mut lhs, &mut self.strings, &mut self.tables);
        self.stack[lhs_idx] = Variant {
            ty: if res { 1 } else { 0 },
            ..Variant::NIL
        };
        Ok(StepOutcome::Continue)
    }

    fn op_vm_or(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let res = lhs.truthy() || rhs.truthy();
        clear_var(&mut rhs, &mut self.strings, &mut self.tables);
        clear_var(&mut lhs, &mut self.strings, &mut self.tables);
        self.stack[lhs_idx] = Variant {
            ty: if res { 1 } else { 0 },
            ..Variant::NIL
        };
        Ok(StepOutcome::Continue)
    }

    fn eq(&self, a: &Variant, b: &Variant) -> bool {
        match (a.ty, b.ty) {
            (2, 2) => (a.value as i32) == (b.value as i32),
            (3, 3) => f32::from_bits(a.value) == f32::from_bits(b.value),
            (2, 3) => (a.value as i32) as f32 == f32::from_bits(b.value),
            (3, 2) => f32::from_bits(a.value) == (b.value as i32) as f32,
            (4, 4) => read_c_string(&self.file.bytes, a.value as usize)
                == read_c_string(&self.file.bytes, b.value as usize),
            (4, 5) => read_c_string(&self.file.bytes, a.value as usize)
                == self.strings.get(b.value).unwrap_or(b""),
            (5, 4) => self.strings.get(a.value).unwrap_or(b"")
                == read_c_string(&self.file.bytes, b.value as usize),
            (5, 5) => self.strings.get(a.value).unwrap_or(b"")
                == self.strings.get(b.value).unwrap_or(b""),
            _ => false,
        }
    }

    fn cmp_strict(&self, a: &Variant, b: &Variant) -> Option<std::cmp::Ordering> {
        match (a.ty, b.ty) {
            (2, 2) => Some((a.value as i32).cmp(&(b.value as i32))),
            (3, 3) => f32::from_bits(a.value).partial_cmp(&f32::from_bits(b.value)),
            (2, 3) => ((a.value as i32) as f32).partial_cmp(&f32::from_bits(b.value)),
            (3, 2) => f32::from_bits(a.value).partial_cmp(&((b.value as i32) as f32)),
            (4, 4) => Some(
                read_c_string(&self.file.bytes, a.value as usize)
                    .cmp(read_c_string(&self.file.bytes, b.value as usize)),
            ),
            (4, 5) => Some(
                read_c_string(&self.file.bytes, a.value as usize)
                    .cmp(self.strings.get(b.value).unwrap_or(b"")),
            ),
            (5, 4) => Some(
                self.strings
                    .get(a.value)
                    .unwrap_or(b"")
                    .cmp(read_c_string(&self.file.bytes, b.value as usize)),
            ),
            (5, 5) => Some(
                self.strings
                    .get(a.value)
                    .unwrap_or(b"")
                    .cmp(self.strings.get(b.value).unwrap_or(b"")),
            ),
            _ => None,
        }
    }

    fn op_vm_sete(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let res = self.eq(&lhs, &rhs);
        clear_var(&mut rhs, &mut self.strings, &mut self.tables);
        clear_var(&mut lhs, &mut self.strings, &mut self.tables);
        self.stack[lhs_idx] = Variant {
            ty: if res { 1 } else { 0 },
            ..Variant::NIL
        };
        Ok(StepOutcome::Continue)
    }

    fn op_vm_setne(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let res = !self.eq(&lhs, &rhs);
        clear_var(&mut rhs, &mut self.strings, &mut self.tables);
        clear_var(&mut lhs, &mut self.strings, &mut self.tables);
        self.stack[lhs_idx] = Variant {
            ty: if res { 1 } else { 0 },
            ..Variant::NIL
        };
        Ok(StepOutcome::Continue)
    }

    fn op_vm_setg(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let res = matches!(
            self.cmp_strict(&lhs, &rhs),
            Some(std::cmp::Ordering::Greater)
        );
        clear_var(&mut rhs, &mut self.strings, &mut self.tables);
        clear_var(&mut lhs, &mut self.strings, &mut self.tables);
        self.stack[lhs_idx] = Variant {
            ty: if res { 1 } else { 0 },
            ..Variant::NIL
        };
        Ok(StepOutcome::Continue)
    }

    fn op_vm_setle(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let res = !matches!(
            self.cmp_strict(&lhs, &rhs),
            Some(std::cmp::Ordering::Greater)
        );
        clear_var(&mut rhs, &mut self.strings, &mut self.tables);
        clear_var(&mut lhs, &mut self.strings, &mut self.tables);
        self.stack[lhs_idx] = Variant {
            ty: if res { 1 } else { 0 },
            ..Variant::NIL
        };
        Ok(StepOutcome::Continue)
    }

    fn op_vm_setl(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let res = matches!(
            self.cmp_strict(&lhs, &rhs),
            Some(std::cmp::Ordering::Less)
        );
        clear_var(&mut rhs, &mut self.strings, &mut self.tables);
        clear_var(&mut lhs, &mut self.strings, &mut self.tables);
        self.stack[lhs_idx] = Variant {
            ty: if res { 1 } else { 0 },
            ..Variant::NIL
        };
        Ok(StepOutcome::Continue)
    }

    fn op_vm_setge(&mut self) -> Result<StepOutcome> {
        let (lhs_idx, mut lhs, mut rhs) = self.bin_take()?;
        let res = !matches!(
            self.cmp_strict(&lhs, &rhs),
            Some(std::cmp::Ordering::Less)
        );
        clear_var(&mut rhs, &mut self.strings, &mut self.tables);
        clear_var(&mut lhs, &mut self.strings, &mut self.tables);
        self.stack[lhs_idx] = Variant {
            ty: if res { 1 } else { 0 },
            ..Variant::NIL
        };
        Ok(StepOutcome::Continue)
    }
}

// Minimal memchr (no external deps).
mod memchr {
    pub fn memchr(needle: u8, haystack: &[u8]) -> Option<usize> {
        for (i, &b) in haystack.iter().enumerate() {
            if b == needle {
                return Some(i);
            }
        }
        None
    }
}
