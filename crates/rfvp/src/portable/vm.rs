use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;

use crate::host_api::RfvpError;

use super::native_bridge::{NativeCallSite, PortableNativeBridge};
use super::parser::Parser;
use super::values::{SavedStackInfo, Table, Variant};

pub type VmResult<T> = core::result::Result<T, VmError>;

const MAX_STACK_SIZE: usize = 0x100;
const CONTEXT_COUNT: usize = 32;

#[derive(Debug, Clone)]
pub enum VmError {
    InvalidData {
        message: String,
        offset: usize,
    },
    Runtime {
        message: String,
        pc: usize,
        thread_id: u32,
    },
    UnsupportedNative {
        syscall_name: String,
        syscall_id: u16,
        pc: usize,
        thread_id: u32,
        reason: String,
    },
    Host(RfvpError),
}

impl VmError {
    pub fn invalid_data(message: &str, offset: usize) -> Self {
        Self::InvalidData {
            message: message.into(),
            offset,
        }
    }

    pub fn runtime(message: impl Into<String>, pc: usize, thread_id: u32) -> Self {
        Self::Runtime {
            message: message.into(),
            pc,
            thread_id,
        }
    }

    pub fn missing_native(
        syscall_name: String,
        syscall_id: u16,
        pc: usize,
        thread_id: u32,
        reason: String,
    ) -> Self {
        Self::UnsupportedNative {
            syscall_name,
            syscall_id,
            pc,
            thread_id,
            reason,
        }
    }

    pub fn to_message(&self) -> String {
        match self {
            Self::InvalidData { message, offset } => {
                format!("invalid script data at offset {:#x}: {}", offset, message)
            }
            Self::Runtime {
                message,
                pc,
                thread_id,
            } => format!(
                "VM runtime error in thread {} at script pc {:#x}: {}",
                thread_id, pc, message
            ),
            Self::UnsupportedNative {
                syscall_name,
                syscall_id,
                pc,
                thread_id,
                reason,
            } => format!(
                "missing no_std native syscall `{}` (id {}) in thread {} at script pc {:#x}: {}",
                syscall_name, syscall_id, thread_id, pc, reason
            ),
            Self::Host(err) => format!("host backend error: {}", err),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadState(u32);

impl ThreadState {
    pub const NONE: Self = Self(0);
    pub const RUNNING: Self = Self(1);
    pub const WAIT: Self = Self(2);
    pub const SLEEP: Self = Self(4);
    pub const TEXT: Self = Self(8);
    pub const DISSOLVE_WAIT: Self = Self(16);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

#[derive(Debug, Clone)]
pub enum ThreadRequest {
    Start(u32, u32),
    Wait(u32),
    Sleep(u32),
    Raise(u32),
    Next,
    TextWait(u32),
    TextResume(u32),
    Exit(Option<u32>),
    ShouldBreak,
}

#[derive(Debug, Clone)]
pub struct PortableVm {
    contexts: Vec<Context>,
    current_id: u32,
    thread_break: bool,
    globals: Globals,
}

#[derive(Debug, Clone, Default)]
struct Globals {
    values: Vec<Variant>,
    non_volatile_count: u16,
    volatile_count: u16,
}

impl PortableVm {
    pub fn new(non_volatile_count: u16, volatile_count: u16) -> Self {
        let mut contexts = Vec::with_capacity(CONTEXT_COUNT);
        for id in 0..CONTEXT_COUNT {
            contexts.push(Context::new(0, id as u32));
        }
        let mut globals = Globals::default();
        globals.init_with(non_volatile_count, volatile_count);
        Self {
            contexts,
            current_id: 0,
            thread_break: false,
            globals,
        }
    }

    pub fn start_main(&mut self, entry_point: u32) {
        self.thread_start(0, entry_point);
    }

    pub fn tick<H: crate::host_api::RfvpHost>(
        &mut self,
        parser: &mut Parser,
        bridge: &mut PortableNativeBridge<'_, H>,
        frame_time_ms: u64,
    ) -> VmResult<()> {
        let total = self.contexts.len() as u32;
        for tid in 0..total {
            self.advance_timers(tid, frame_time_ms);
            let status = self.context_status(tid);
            if status.contains(ThreadState::RUNNING)
                && !status.contains(ThreadState::WAIT)
                && !status.contains(ThreadState::SLEEP)
                && !status.contains(ThreadState::TEXT)
                && !status.contains(ThreadState::DISSOLVE_WAIT)
            {
                self.run_one_context(tid, parser, bridge)?;
            }
        }
        Ok(())
    }

    pub fn thread_break(&self) -> bool {
        self.thread_break
    }

    fn run_one_context<H: crate::host_api::RfvpHost>(
        &mut self,
        tid: u32,
        parser: &mut Parser,
        bridge: &mut PortableNativeBridge<'_, H>,
    ) -> VmResult<()> {
        self.current_id = tid;
        self.contexts[tid as usize].should_break = false;
        while !self.contexts[tid as usize].should_break {
            self.contexts[tid as usize].dispatch_opcode(
                parser,
                bridge,
                &mut self.globals,
            )?;

            if self.contexts[tid as usize].should_exit {
                self.thread_exit(Some(tid));
                break;
            }

            let mut must_yield = false;
            for request in bridge.take_requests() {
                match request {
                    ThreadRequest::Start(id, addr) => self.thread_start(id, addr),
                    ThreadRequest::Wait(time) => {
                        self.thread_wait(time);
                        must_yield = true;
                    }
                    ThreadRequest::Sleep(time) => {
                        self.thread_sleep(time);
                        must_yield = true;
                    }
                    ThreadRequest::Raise(time) => self.thread_raise(time),
                    ThreadRequest::Next => {
                        self.contexts[tid as usize].should_break = true;
                        must_yield = true;
                    }
                    ThreadRequest::TextWait(id) => {
                        let mut st = self.context_status(id);
                        st.insert(ThreadState::TEXT);
                        st.remove(ThreadState::RUNNING);
                        self.set_context_status(id, st);
                        must_yield = true;
                    }
                    ThreadRequest::TextResume(id) => {
                        let mut st = self.context_status(id);
                        st.remove(ThreadState::TEXT);
                        st.insert(ThreadState::RUNNING);
                        self.set_context_status(id, st);
                    }
                    ThreadRequest::Exit(id) => {
                        self.thread_exit(id);
                        must_yield = true;
                    }
                    ThreadRequest::ShouldBreak => {
                        self.contexts[tid as usize].should_break = true;
                        self.thread_break = true;
                        must_yield = true;
                    }
                }
            }
            if must_yield {
                break;
            }
        }
        Ok(())
    }

    fn advance_timers(&mut self, tid: u32, frame_time_ms: u64) {
        let status = self.context_status(tid);
        if status.contains(ThreadState::WAIT) {
            let ctx = &mut self.contexts[tid as usize];
            if ctx.wait_ms > frame_time_ms {
                ctx.wait_ms -= frame_time_ms;
            } else {
                ctx.wait_ms = 0;
                ctx.state.remove(ThreadState::WAIT);
                ctx.state.insert(ThreadState::RUNNING);
            }
        }
        if status.contains(ThreadState::SLEEP) {
            let ctx = &mut self.contexts[tid as usize];
            if ctx.sleep_ms > frame_time_ms {
                ctx.sleep_ms -= frame_time_ms;
            } else {
                ctx.sleep_ms = 0;
                ctx.state.remove(ThreadState::SLEEP);
                ctx.state.insert(ThreadState::RUNNING);
            }
        }
    }

    fn context_status(&self, id: u32) -> ThreadState {
        self.contexts[id as usize].state
    }

    fn set_context_status(&mut self, id: u32, status: ThreadState) {
        self.contexts[id as usize].state = status;
    }

    fn thread_start(&mut self, id: u32, addr: u32) {
        if id as usize >= self.contexts.len() {
            return;
        }
        if id == 0 {
            self.thread_break = false;
            for i in 0..self.contexts.len() {
                let mut context = Context::new(0, i as u32);
                context.state = ThreadState::NONE;
                context.should_break = true;
                self.contexts[i] = context;
            }
        }
        let mut context = Context::new(addr, id);
        context.state = ThreadState::RUNNING;
        self.contexts[id as usize] = context;
    }

    fn thread_wait(&mut self, time: u32) {
        let ctx = &mut self.contexts[self.current_id as usize];
        ctx.should_break = true;
        ctx.wait_ms = time as u64;
        ctx.state.insert(ThreadState::WAIT);
        ctx.state.remove(ThreadState::RUNNING);
    }

    fn thread_sleep(&mut self, time: u32) {
        let ctx = &mut self.contexts[self.current_id as usize];
        ctx.should_break = true;
        ctx.sleep_ms = time as u64;
        ctx.state.insert(ThreadState::SLEEP);
        ctx.state.remove(ThreadState::RUNNING);
    }

    fn thread_raise(&mut self, time: u32) {
        for ctx in &mut self.contexts {
            if ctx.state.contains(ThreadState::SLEEP) && ctx.wait_ms == time as u64 {
                ctx.state.remove(ThreadState::SLEEP);
                ctx.state.insert(ThreadState::RUNNING);
            }
        }
    }

    fn thread_exit(&mut self, id: Option<u32>) {
        let id = id.unwrap_or(self.current_id);
        if id as usize >= self.contexts.len() {
            return;
        }
        if id == 0 {
            for i in 0..self.contexts.len() {
                let mut ctx = Context::new(0, i as u32);
                ctx.state = ThreadState::NONE;
                ctx.should_break = true;
                self.contexts[i] = ctx;
            }
            self.thread_break = true;
        } else {
            let mut ctx = Context::new(0, id);
            ctx.state = ThreadState::NONE;
            ctx.should_break = true;
            self.contexts[id as usize] = ctx;
        }
    }
}

impl Globals {
    fn init_with(&mut self, non_volatile: u16, volatile: u16) {
        self.non_volatile_count = non_volatile;
        self.volatile_count = volatile;
        self.values.clear();
        self.values
            .resize(non_volatile.saturating_add(volatile) as usize, Variant::Nil);
    }

    fn get(&self, key: u16) -> Variant {
        self.values
            .get(key as usize)
            .cloned()
            .unwrap_or(Variant::Nil)
    }

    fn set(&mut self, key: u16, value: Variant) {
        let key = key as usize;
        if key >= self.values.len() {
            self.values.resize(key + 1, Variant::Nil);
        }
        self.values[key] = value;
    }

    fn get_mut_or_nil(&mut self, key: u16) -> &mut Variant {
        let key = key as usize;
        if key >= self.values.len() {
            self.values.resize(key + 1, Variant::Nil);
        }
        &mut self.values[key]
    }
}

#[derive(Debug, Clone)]
struct Context {
    id: u32,
    stack: Vec<Variant>,
    cursor: usize,
    cur_stack_pos: usize,
    cur_stack_base: usize,
    return_value: Variant,
    state: ThreadState,
    wait_ms: u64,
    sleep_ms: u64,
    should_exit: bool,
    should_break: bool,
}

impl Context {
    fn new(start_addr: u32, id: u32) -> Self {
        let mut ctx = Self {
            id,
            stack: vec![Variant::Nil; MAX_STACK_SIZE],
            cursor: start_addr as usize,
            cur_stack_pos: 0,
            cur_stack_base: 0,
            return_value: Variant::Nil,
            state: ThreadState::NONE,
            wait_ms: 0,
            sleep_ms: 0,
            should_exit: false,
            should_break: false,
        };
        let _ = ctx.push(Variant::SavedStackInfo(SavedStackInfo {
            stack_base: 0,
            stack_pos: 0,
            return_addr: usize::MAX,
            args: 0,
        }));
        ctx.cur_stack_base = ctx.cur_stack_pos;
        ctx.cur_stack_pos = 0;
        ctx
    }

    fn dispatch_opcode<H: crate::host_api::RfvpHost>(
        &mut self,
        parser: &mut Parser,
        bridge: &mut PortableNativeBridge<'_, H>,
        globals: &mut Globals,
    ) -> VmResult<()> {
        let opcode = parser.read_u8(self.cursor)?;
        match opcode {
            0x00 => self.nop(),
            0x01 => self.init_stack(parser),
            0x02 => self.call(parser),
            0x03 => self.syscall(parser, bridge),
            0x04 => self.ret(false),
            0x05 => self.ret(true),
            0x06 => self.jmp(parser),
            0x07 => self.jz(parser),
            0x08 => self.push_nil(),
            0x09 => self.push_true(),
            0x0a => self.push_i32(parser),
            0x0b => self.push_i16(parser),
            0x0c => self.push_i8(parser),
            0x0d => self.push_f32(parser),
            0x0e => self.push_string(parser),
            0x0f => self.push_global(parser, globals),
            0x10 => self.push_stack(parser),
            0x11 => self.push_global_table(parser, globals),
            0x12 => self.push_local_table(parser),
            0x13 => self.push_top(),
            0x14 => self.push_return_value(),
            0x15 => self.pop_global(parser, globals),
            0x16 => self.local_copy(parser),
            0x17 => self.pop_global_table(parser, globals),
            0x18 => self.pop_local_table(parser),
            0x19 => self.unary_neg(),
            0x1a => self.binary_op(Variant::add),
            0x1b => self.binary_op(Variant::sub),
            0x1c => self.binary_op(Variant::mul),
            0x1d => self.binary_op(Variant::div),
            0x1e => self.binary_op(Variant::modulo),
            0x1f => self.binary_op(Variant::bit_test),
            0x20 => self.binary_op(Variant::and),
            0x21 => self.binary_op(Variant::or),
            0x22 => self.binary_op(Variant::equal),
            0x23 => self.binary_op(Variant::not_equal),
            0x24 => self.binary_op(Variant::greater),
            0x25 => self.binary_op(Variant::greater_equal),
            0x26 => self.binary_op(Variant::less),
            0x27 => self.binary_op(Variant::less_equal),
            _ => Err(self.err(format!("unknown opcode {:#x}", opcode))),
        }
    }

    fn err(&self, message: impl Into<String>) -> VmError {
        VmError::runtime(message, self.cursor, self.id)
    }

    fn to_global_offset(&self) -> VmResult<usize> {
        self.cur_stack_base
            .checked_add(self.cur_stack_pos)
            .ok_or_else(|| self.err("stack pointer overflow"))
    }

    fn push(&mut self, value: Variant) -> VmResult<()> {
        let pos = self.to_global_offset()?;
        if pos >= self.stack.len() {
            return Err(self.err("stack overflow"));
        }
        self.stack[pos] = value;
        self.cur_stack_pos += 1;
        Ok(())
    }

    fn pop(&mut self) -> VmResult<Variant> {
        if self.cur_stack_pos == 0 {
            return Err(self.err("stack underflow"));
        }
        let pos = self.to_global_offset()?.saturating_sub(1);
        let out = self.stack.get(pos).cloned().ok_or_else(|| self.err("stack pop out of bounds"))?;
        self.stack[pos].set_nil();
        self.cur_stack_pos -= 1;
        Ok(out)
    }

    fn local_index(&self, offset: i8) -> VmResult<usize> {
        let base = self.cur_stack_base as isize + offset as isize;
        if base < 0 || base as usize >= self.stack.len() {
            return Err(self.err("local stack access out of bounds"));
        }
        Ok(base as usize)
    }

    fn get_local(&self, offset: i8) -> VmResult<Variant> {
        Ok(self.stack[self.local_index(offset)?].clone())
    }

    fn get_local_mut(&mut self, offset: i8) -> VmResult<&mut Variant> {
        let idx = self.local_index(offset)?;
        Ok(&mut self.stack[idx])
    }

    fn set_local(&mut self, offset: i8, value: Variant) -> VmResult<()> {
        let idx = self.local_index(offset)?;
        self.stack[idx] = value;
        Ok(())
    }

    fn nop(&mut self) -> VmResult<()> {
        self.cursor += 1;
        Ok(())
    }

    fn init_stack(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let args_count = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        let locals_count = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        if args_count < 0 || locals_count < 0 {
            return Err(self.err("negative init_stack count"));
        }
        let frame = self.get_local_mut(-1)?;
        let Some(frame) = frame.as_saved_stack_info_mut() else {
            return Err(self.err("init_stack found invalid stack frame"));
        };
        frame.args = args_count as usize;
        for _ in 0..locals_count {
            self.push(Variant::Nil)?;
        }
        Ok(())
    }

    fn call(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let addr = parser.read_u32(self.cursor)?;
        self.cursor += size_of::<u32>();
        if !parser.is_code_area(addr) {
            return Err(self.err(format!("call target {:#x} outside code area", addr)));
        }
        self.push(Variant::SavedStackInfo(SavedStackInfo {
            stack_base: self.cur_stack_base,
            stack_pos: self.cur_stack_pos,
            return_addr: self.cursor,
            args: 0,
        }))?;
        self.cur_stack_base += self.cur_stack_pos;
        self.cur_stack_pos = 0;
        self.cursor = addr as usize;
        Ok(())
    }

    fn syscall<H: crate::host_api::RfvpHost>(
        &mut self,
        parser: &Parser,
        bridge: &mut PortableNativeBridge<'_, H>,
    ) -> VmResult<()> {
        self.cursor += 1;
        let syscall_id = parser.read_u16(self.cursor)?;
        let call_pc = self.cursor - 1;
        self.cursor += size_of::<u16>();
        let Some(syscall) = parser.get_syscall(syscall_id) else {
            return Err(self.err(format!("syscall id {} not found", syscall_id)));
        };
        let mut args = Vec::new();
        for _ in 0..syscall.args {
            args.push(self.pop()?);
        }
        args.reverse();
        let call_site = NativeCallSite {
            thread_id: self.id,
            pc: call_pc,
            syscall_id,
            syscall_name: syscall.name.clone(),
        };
        self.return_value = bridge.syscall(call_site, args)?;
        Ok(())
    }

    fn ret(&mut self, with_value: bool) -> VmResult<()> {
        self.cursor += 1;
        self.return_value = if with_value { self.pop()? } else { Variant::Nil };
        let frame = self.get_local(-1)?;
        let Some(frame) = frame.as_saved_stack_info() else {
            return Err(self.err("return found invalid stack frame"));
        };
        let frame = frame.clone();
        self.cur_stack_pos = frame.stack_pos;
        self.cur_stack_base = frame.stack_base;
        self.cursor = frame.return_addr;
        if self.cursor == usize::MAX {
            self.should_exit = true;
            return Ok(());
        }
        for _ in 0..frame.args {
            self.pop()?;
        }
        Ok(())
    }

    fn jmp(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        self.cursor = parser.read_u32(self.cursor)? as usize;
        Ok(())
    }

    fn jz(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let addr = parser.read_u32(self.cursor)?;
        self.cursor += size_of::<u32>();
        let top = self.pop()?;
        if !top.canbe_true() {
            self.cursor = addr as usize;
        }
        Ok(())
    }

    fn push_nil(&mut self) -> VmResult<()> {
        self.cursor += 1;
        self.push(Variant::Nil)
    }

    fn push_true(&mut self) -> VmResult<()> {
        self.cursor += 1;
        self.push(Variant::True)
    }

    fn push_i32(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let value = parser.read_i32(self.cursor)?;
        self.cursor += size_of::<i32>();
        self.push(Variant::Int(value))
    }

    fn push_i16(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let value = parser.read_i16(self.cursor)?;
        self.cursor += size_of::<i16>();
        self.push(Variant::Int(value as i32))
    }

    fn push_i8(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let value = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        self.push(Variant::Int(value as i32))
    }

    fn push_f32(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let value = parser.read_f32(self.cursor)?;
        self.cursor += size_of::<f32>();
        self.push(Variant::Float(value))
    }

    fn push_string(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let len = parser.read_u8(self.cursor)? as usize;
        self.cursor += size_of::<u8>();
        let addr = self.cursor as u32;
        let value = parser.read_cstring(self.cursor, len)?;
        self.cursor += len;
        self.push(Variant::ConstString(value, addr))
    }

    fn push_global(&mut self, parser: &Parser, globals: &mut Globals) -> VmResult<()> {
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();
        self.push(globals.get(key))
    }

    fn push_stack(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let offset = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        self.push(self.get_local(offset)?)
    }

    fn push_global_table(&mut self, parser: &Parser, globals: &mut Globals) -> VmResult<()> {
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();
        let index = self.pop()?.as_int().unwrap_or(0) as u32;
        let value = match globals.get_mut_or_nil(key) {
            Variant::Table(table) => table.get(index).cloned().unwrap_or(Variant::Nil),
            _ => Variant::Nil,
        };
        self.push(value)
    }

    fn push_local_table(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let offset = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        let index = self.pop()?.as_int().unwrap_or(0) as u32;
        let value = match self.get_local_mut(offset)? {
            Variant::Table(table) => table.get(index).cloned().unwrap_or(Variant::Nil),
            _ => Variant::Nil,
        };
        self.push(value)
    }

    fn push_top(&mut self) -> VmResult<()> {
        self.cursor += 1;
        let value = if self.cur_stack_pos == 0 {
            Variant::Nil
        } else {
            self.stack[self.to_global_offset()?.saturating_sub(1)].clone()
        };
        self.push(value)
    }

    fn push_return_value(&mut self) -> VmResult<()> {
        self.cursor += 1;
        self.push(self.return_value.clone())
    }

    fn pop_global(&mut self, parser: &Parser, globals: &mut Globals) -> VmResult<()> {
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();
        let value = self.pop()?;
        globals.set(key, value);
        Ok(())
    }

    fn local_copy(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let offset = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        let value = self.pop()?;
        self.set_local(offset, value)
    }

    fn pop_global_table(&mut self, parser: &Parser, globals: &mut Globals) -> VmResult<()> {
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();
        let index = self.pop()?.as_int().unwrap_or(0) as u32;
        let value = self.pop()?;
        let slot = globals.get_mut_or_nil(key);
        if !matches!(slot, Variant::Table(_)) {
            *slot = Variant::Table(Table::new());
        }
        if let Some(table) = slot.as_table_mut() {
            table.insert(index, value);
        }
        Ok(())
    }

    fn pop_local_table(&mut self, parser: &Parser) -> VmResult<()> {
        self.cursor += 1;
        let offset = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        let index = self.pop()?.as_int().unwrap_or(0) as u32;
        let value = self.pop()?;
        let slot = self.get_local_mut(offset)?;
        if !matches!(slot, Variant::Table(_)) {
            *slot = Variant::Table(Table::new());
        }
        if let Some(table) = slot.as_table_mut() {
            table.insert(index, value);
        }
        Ok(())
    }

    fn unary_neg(&mut self) -> VmResult<()> {
        self.cursor += 1;
        let mut value = self.pop()?;
        value.neg();
        self.push(value)
    }

    fn binary_op(&mut self, op: fn(&mut Variant, &Variant)) -> VmResult<()> {
        self.cursor += 1;
        let rhs = self.pop()?;
        let mut lhs = self.pop()?;
        op(&mut lhs, &rhs);
        self.push(lhs)
    }
}
