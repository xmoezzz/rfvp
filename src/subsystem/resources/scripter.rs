use crate::script::context::Context;
use std::collections::HashMap;

#[derive(Default)]
pub struct ScriptScheduler {
    queue: HashMap<u32, Context>,
    // which is the current context
    current_id: u32,
}

impl ScriptScheduler {
    pub fn new() -> Self {
        ScriptScheduler {
            queue: HashMap::new(),
            current_id: 0,
        }
    }

    pub fn add(&mut self, id: u32, context: Context) {
        self.queue.insert(id, context);
    }

    pub fn remove(&mut self, id: u32) {
        self.queue.remove(&id);
    }

    pub fn get_current_thread(&mut self) -> Option<&mut Context> {
        self.queue.get_mut(&self.current_id)
    }

    // pub fn start_main(&mut self, entry_point: u32) {
    //     self.thread_start(0, entry_point).unwrap();
    // }
}
