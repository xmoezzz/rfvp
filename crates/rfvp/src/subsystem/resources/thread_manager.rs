use std::cell::{RefCell, RefMut};

use crate::script::{context::{Context, ThreadState}, parser::Parser, VmSyscall};

#[derive(Debug)]
pub struct ThreadManager {
    pub contexts: Vec<Context>,
    current_id: u32,
    thread_break: bool,
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadManager {
    pub fn new() -> Self {
        let mut contexts = Vec::with_capacity(32);
        for i in 0..32 {
            let context = Context::new(0, i as u32);
            contexts.push(context);
        }
        
        ThreadManager {
            contexts,
            current_id: 0,
            thread_break: false,
        }
    }

    pub fn get_current_id(&self) -> u32 {
        self.current_id
    }

    pub fn get_current_id_mut(&mut self) -> &mut u32 {
        &mut self.current_id
    }

    pub fn set_current_id(&mut self, id: u32) {
        self.current_id = id;
    }

    pub fn total_contexts(&self) -> usize {
        self.contexts.len()
    }

    pub fn get_context_status(&self, id: u32) -> ThreadState {
        self.contexts[id as usize].get_status()
    }

    pub fn set_context_status(&mut self, id: u32, status: ThreadState) {
        self.contexts[id as usize].set_status(status);
    }

    pub fn get_context_waiting_time(&self, id: u32) -> u64 {
        self.contexts[id as usize].get_waiting_time()
    }

    pub fn set_context_waiting_time(&mut self, id: u32, time: u64) {
        self.contexts[id as usize].set_waiting_time(time);
    }

    pub fn get_context_sleeping_time(&self, id: u32) -> u64 {
        self.contexts[id as usize].get_sleeping_time()
    }

    pub fn set_context_sleeping_time(&mut self, id: u32, time: u64) {
        self.contexts[id as usize].set_sleeping_time(time);
    }

    pub fn set_context_should_break(&mut self, id: u32, should_break: bool) {
        self.contexts[id as usize].set_should_break(should_break);
    }

    pub fn get_context_should_break(&self, id: u32) -> bool {
        self.contexts[id as usize].should_break()
    }

    pub fn get_contexct_should_exit(&self, id: u32) -> bool {
        self.contexts[id as usize].should_exit_now()
    }

    pub fn context_dispatch_opcode(&mut self, id: u32, syscaller: &mut impl VmSyscall, parser: &mut Parser) -> anyhow::Result<()> {
        self.contexts[id as usize].dispatch_opcode(syscaller, parser)
    }

    pub fn get_should_break(&self) -> bool {
        self.thread_break
    }

    pub fn set_should_break(&mut self, should_break: bool) {
        self.thread_break = should_break;
    }

    pub fn thread_start(&mut self, id: u32, addr: u32) {
        if id == 0 {
            // Reset the global break flag when restarting the main thread.
            self.thread_break = false;
            for i in 0..self.total_contexts() {
                let mut context = Context::new(0, i as u32);
                context.set_status(ThreadState::CONTEXT_STATUS_NONE);
                context.set_should_break(true);
                self.contexts[i] = context;
            }
        }

        let mut context = Context::new(addr, id);
        context.set_status(ThreadState::CONTEXT_STATUS_RUNNING);
        self.contexts[id as usize] = context;
    }

    pub fn thread_wait(&mut self, time: u32) {
        self.contexts[self.current_id as usize].set_should_break(true);
        self.contexts[self.current_id as usize].set_waiting_time(time as u64);

        let status = self.contexts[self.current_id as usize].get_status();
        // WAIT blocks execution: clear RUNNING until the timer expires.
        self.contexts[self.current_id as usize]
            .set_status((status | ThreadState::CONTEXT_STATUS_WAIT) & !ThreadState::CONTEXT_STATUS_RUNNING);
    }

    pub fn thread_dissolve_wait(&mut self) {
        self.contexts[self.current_id as usize].set_should_break(true);
        let status = self.contexts[self.current_id as usize].get_status();
        // DISSOLVE_WAIT blocks execution: clear RUNNING until dissolve completes.
        self.contexts[self.current_id as usize].set_status(
            (status | ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT) & !ThreadState::CONTEXT_STATUS_RUNNING,
        );
    }

    pub fn thread_sleep(&mut self, time: u32) {
        self.contexts[self.current_id as usize].set_should_break(true);
        self.contexts[self.current_id as usize].set_sleeping_time(time as u64);

        let status = self.contexts[self.current_id as usize].get_status();
        // SLEEP blocks execution: clear RUNNING until raised/unblocked.
        self.contexts[self.current_id as usize]
            .set_status((status | ThreadState::CONTEXT_STATUS_SLEEP) & !ThreadState::CONTEXT_STATUS_RUNNING);
    }

    pub fn thread_raise(&mut self, time: u32) {
        for i in 0..self.total_contexts() {
            let status = self.contexts[i].get_status();
            // wtf?
            // both sleep and raise are never used
            if status.contains(ThreadState::CONTEXT_STATUS_SLEEP) && self.contexts[i].get_waiting_time() == time as u64 {
                self.contexts[i].set_status((status & !ThreadState::CONTEXT_STATUS_SLEEP) | ThreadState::CONTEXT_STATUS_RUNNING);
            }
        }
    }

    pub fn thread_next(&mut self) {
        self.contexts[self.current_id as usize].set_should_break(true);
    }

    pub fn thread_exit(&mut self, id: Option<u32>) {
        let id = match id {
            Some(id) => id,
            None => self.current_id,
        };

        if id == 0 {
            for i in 0..self.total_contexts() {
                let mut ctx = Context::new(0, i as u32);
                ctx.set_status(ThreadState::CONTEXT_STATUS_NONE);
                ctx.set_should_break(true);
                self.contexts[i] = ctx;
            }

            self.thread_break = true;

        } else {
            let mut ctx = Context::new(0, id);
            ctx.set_status(ThreadState::CONTEXT_STATUS_NONE);
            ctx.set_should_break(true);
            self.contexts[id as usize] = ctx;
        }
    }

    pub fn start_main(&mut self, entry_point: u32) {
        self.thread_start(0, entry_point);
    }
}
