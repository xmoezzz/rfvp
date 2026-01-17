use anyhow::Result;

use crate::script::{context::ThreadState, parser::Parser};
use crate::subsystem::resources::{
    motion_manager::DissolveType,
    thread_manager::ThreadManager,
    thread_wrapper::ThreadRequest,
};
use crate::subsystem::world::GameData;

/// Drives the script VM (which is coroutine-based, not OS-thread based).
///
/// Design goal: isolate context-switching and opcode execution from the rest of the engine loop,
/// while keeping the refactor minimally invasive.
#[derive(Debug)]
pub struct VmRunner {
    tm: ThreadManager,
}

impl VmRunner {
    pub fn new(tm: ThreadManager) -> Self {
        Self { tm }
    }

    pub fn thread_manager(&self) -> &ThreadManager {
        &self.tm
    }

    pub fn thread_manager_mut(&mut self) -> &mut ThreadManager {
        &mut self.tm
    }

    pub fn start_main(&mut self, entry_point: u32) {
        self.tm.start_main(entry_point);
    }

    /// Execute one engine frame worth of script VM work.
    ///
    /// `frame_time_ms` is the elapsed time budget for timers (wait/sleep/etc.).
    pub fn tick(&mut self, game: &mut GameData, parser: &mut Parser, frame_time_ms: u64) -> Result<()> {
        // The VM itself is cooperative; the engine decides when to advance contexts.
        // If the game is halted (e.g. waiting for IO / modal UI), we do not advance contexts.
        if game.get_halt() {
            return Ok(());
        }

        // In the original engine, dissolve is a global visual state that can unblock VM waits.
        let dissolve_type = game.motion_manager.get_dissolve_type();

        // Hard cap of opcode dispatches per frame to avoid the VM monopolizing the engine loop.
        // This is critical for games that spin in script (polling input, timers, etc.).
        // The original engine is cooperative; we must enforce cooperation even if a syscall
        // forgets to yield.
        let max_ops_per_context: usize = std::env::var("RFVP_VM_MAX_OPS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(2000);

        let total = self.tm.total_contexts() as u32;
        for tid in 0..total {
            if game.get_halt() {
                break;
            }

            // Legacy behavior: when the game is terminating, only the last-running context keeps advancing.
            if game.get_game_should_exit() && game.get_last_current_thread() != tid {
                continue;
            }

            self.advance_timers_and_state(tid, dissolve_type, frame_time_ms);

            let status = self.tm.get_context_status(tid);
            if status.contains(ThreadState::CONTEXT_STATUS_RUNNING)
                && !status.contains(ThreadState::CONTEXT_STATUS_WAIT)
                && !status.contains(ThreadState::CONTEXT_STATUS_SLEEP)
                && !status.contains(ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT)
            {
                self.run_one_context(tid, game, parser, max_ops_per_context)?;
            }
        }

        Ok(())
    }

    fn advance_timers_and_state(&mut self, tid: u32, dissolve_type: DissolveType, frame_time_ms: u64) {
        let status = self.tm.get_context_status(tid);

        // WAIT timer
        if status.contains(ThreadState::CONTEXT_STATUS_WAIT) {
            let wait_time = self.tm.get_context_waiting_time(tid);
            if wait_time > frame_time_ms {
                self.tm
                    .set_context_waiting_time(tid, wait_time - frame_time_ms);
            } else {
                self.tm.set_context_waiting_time(tid, 0);
                let mut new_status = status.clone();
                new_status.remove(ThreadState::CONTEXT_STATUS_WAIT);
                // Resume execution once WAIT expires.
                new_status.insert(ThreadState::CONTEXT_STATUS_RUNNING);
                self.tm.set_context_status(tid, new_status);
            }
        }

        // Dissolve wait is unblocked when dissolve is completed / static.
        if status.contains(ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT)
            && (dissolve_type == DissolveType::None || dissolve_type == DissolveType::Static)
        {
            let mut new_status = status.clone();
            new_status.remove(ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT);
            // Resume execution once dissolve completes/static.
            new_status.insert(ThreadState::CONTEXT_STATUS_RUNNING);
            self.tm.set_context_status(tid, new_status);
        }
    }

    fn run_one_context(
        &mut self,
        tid: u32,
        game: &mut GameData,
        parser: &mut Parser,
        mut opcode_budget: usize,
    ) -> Result<()> {
        // Keep the VM's notion of current thread aligned with GameData.
        self.tm.set_current_id(tid);
        game.set_current_thread(tid);

        self.tm.set_context_should_break(tid, false);
        while !self.tm.get_context_should_break(tid) {
            if opcode_budget == 0 {
                // Force-yield to keep the engine responsive.
                self.tm.set_context_should_break(tid, true);
                break;
            }

            let result = self.tm.context_dispatch_opcode(tid, game, parser);
            opcode_budget -= 1;

            if self.tm.get_contexct_should_exit(tid) {
                self.tm.thread_exit(Some(tid));
                break;
            }

            if let Err(e) = result {
                // Preserve the previous "fail fast" behavior for now.
                log::error!("Error while executing the script: {:#?}", e);
                anyhow::bail!(e);
            }

            // Drain all pending requests emitted by syscalls.
            while let Some(event) = game.thread_wrapper.peek() {
                match event {
                    ThreadRequest::Start(id, addr) => self.tm.thread_start(id, addr),
                    ThreadRequest::Wait(time) => self.tm.thread_wait(time),
                    ThreadRequest::DissolveWait() => self.tm.thread_dissolve_wait(),
                    ThreadRequest::Sleep(time) => self.tm.thread_sleep(time),
                    ThreadRequest::Raise(time) => self.tm.thread_raise(time),
                    ThreadRequest::Next() => self.tm.thread_next(),
                    ThreadRequest::Exit(id) => self.tm.thread_exit(id),
                    ThreadRequest::ShouldBreak() => {
                        // Must break the CURRENT context, not a global flag.
                        self.tm.set_context_should_break(tid, true);
                        self.tm.set_should_break(true);
                    }
                }
            }
        }

        Ok(())
    }
}
