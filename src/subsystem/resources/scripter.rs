use crate::script::global::Global;
use crate::script::parser::Parser;
use crate::script::context::Context;
use crate::subsystem::world::GameData;
use std::collections::HashMap;

use anyhow::{bail, Result};

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

    pub fn get(&self, id: u32) -> Option<&Context> {
        self.queue.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Context> {
        self.queue.get_mut(&id)
    }

    /// move to the next schedulable context
    fn switch_context(&mut self) {
        let mut keys = self.queue.keys().map(|k| *k).collect::<Vec<_>>();
        keys.sort();

        let idx = keys.binary_search(&self.current_id).unwrap();

        let (left, middle_right) = keys.split_at(idx);
        let (middle, right) = middle_right.split_at(1);

        let mut ids = Vec::new();
        ids.extend_from_slice(right);
        ids.extend_from_slice(left);

        for id in ids {
            if let Some(context) = self.queue.get_mut(&id) {
                if context.is_running() || context.is_yielded() {
                    self.current_id = id;
                    return;
                }
            }
        }
    }

    fn current_context(&mut self) -> Result<&mut Context> {
        if let Some(context) = self.queue.get_mut(&self.current_id) {
            return Ok(context);
        }

        bail!("no context found");
    }

    pub fn elapsed(&mut self, elapsed_ms: u64) {
        for (_, context) in self.queue.iter_mut() {
            context.elapsed(elapsed_ms);
        }
    }

    pub fn execute(
        &mut self,
        rendering_time: u64,
        total_time: u64,
        game_data: &GameData,
        parser: &mut Parser,
        global: &mut Global,
    ) -> Result<()> {
        let current_time = std::time::Instant::now();

        // let parser = self.get_parser_mut();
        let context = self.current_context()?;
        if context.is_yielded() || context.is_suspended() {
            if context.is_yielded() {
                // next time, context will be schedulable
                context.set_running();
            }
            // make sure all the waitable contexts have to set corsponding elapsed time
            let script_elapsed = current_time.elapsed().as_millis() as u64;
            self.elapsed(script_elapsed + rendering_time);
            // set the next schedulable context
            self.switch_context();
            // give up the current time slice
            return Ok(());
        }

        loop {
            // do less syscall as possible during the script execution
            if current_time.elapsed().as_millis() >= total_time.into() {
                // we have reached the time limit
                break;
            }

            // we execute the script 50 instructions at a time
            for _ in 0..50 {
                context.dispatch_opcode(game_data, parser, global)?;

                // in case the script is triggered to yield or sleep
                if context.is_yielded() || context.is_suspended() {
                    // elapsed time for all the contexts
                    let script_elapsed = current_time.elapsed().as_millis() as u64;
                    self.elapsed(script_elapsed + rendering_time);
                    return Ok(());
                }
            }
        }

        // elapsed time for all the contexts
        let script_elapsed = current_time.elapsed().as_millis() as u64;
        self.elapsed(script_elapsed + rendering_time);

        Ok(())
    }
}
