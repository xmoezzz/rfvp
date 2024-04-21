use std::cell::{RefCell, RefMut};

use crate::script::context::{Context, CONTEXT_STATUS_NONE, CONTEXT_STATUS_RUNNING, CONTEXT_STATUS_SLEEP, CONTEXT_STATUS_WAIT};

pub struct ThreadManager {
    pub contexts: Vec<RefCell<Context>>,
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
        ThreadManager {
            contexts: vec![RefCell::new(Context::new(0)); 32],
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

    pub fn get_thread(&mut self, id: u32) -> RefMut<'_, Context> {
        self.contexts[id as usize].borrow_mut()
    }

    pub fn get_should_break(&self) -> bool {
        self.thread_break
    }

    pub fn set_should_break(&mut self, should_break: bool) {
        self.thread_break = should_break;
    }

    pub fn thread_start(&mut self, id: u32, addr: u32) {
        if id == 0 {
            for _i in 0..self.total_contexts() {
                let mut context = Context::new(0);
                context.set_status(CONTEXT_STATUS_NONE);
                context.set_should_break(true);
                self.contexts[id as usize] = RefCell::new(context);
            }
        }

        let mut context = Context::new(addr);
        context.set_status(CONTEXT_STATUS_RUNNING);
        self.contexts[id as usize] = RefCell::new(context);
    }

    pub fn thread_wait(&mut self, time: u32) {
        self.contexts[self.current_id as usize].borrow_mut().set_should_break(true);
        self.contexts[self.current_id as usize].borrow_mut().set_waiting_time(time as u64);

        let status = self.contexts[self.current_id as usize].borrow_mut().get_status();
        self.contexts[self.current_id as usize].borrow_mut().set_status(status | CONTEXT_STATUS_WAIT);
    }

    pub fn thread_sleep(&mut self, time: u32) {
        self.contexts[self.current_id as usize].borrow_mut().set_should_break(true);
        self.contexts[self.current_id as usize].borrow_mut().set_waiting_time(time as u64);

        let status = self.contexts[self.current_id as usize].borrow_mut().get_status();
        self.contexts[self.current_id as usize].borrow_mut().set_status(status | CONTEXT_STATUS_SLEEP);
    }

    pub fn thread_raise(&mut self, time: u32) {
        for i in 0..self.total_contexts() {
            let status = self.contexts[i].borrow_mut().get_status();
            // wtf?
            // both sleep and raise are never used
            if status & CONTEXT_STATUS_SLEEP != 0 && self.contexts[i].borrow_mut().get_waiting_time() == time as u64 {
                self.contexts[i].borrow_mut().set_status(status & !CONTEXT_STATUS_SLEEP);
            }
        }
    }

    pub fn thread_next(&mut self) {
        self.contexts[self.current_id as usize].borrow_mut().set_should_break(true);
    }

    pub fn thread_exit(&mut self, id: Option<u32>) {
        let id = match id {
            Some(id) => id,
            None => self.current_id,
        };

        if id == 0 {
            for _i in 0..self.total_contexts() {
                let mut ctx = Context::new(0);
                ctx.set_status(CONTEXT_STATUS_NONE);
                ctx.set_should_break(true);
                self.contexts[id as usize] = RefCell::new(ctx);
            }

            self.thread_break = true;

        } else {
            let mut ctx = Context::new(0);
            ctx.set_status(CONTEXT_STATUS_NONE);
            ctx.set_should_break(true);
            self.contexts[id as usize] = RefCell::new(ctx);
        }
    }

    pub fn start_main(&mut self, entry_point: u32) {
        self.thread_start(0, entry_point);
    }
}
