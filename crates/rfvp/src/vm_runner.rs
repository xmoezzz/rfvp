use anyhow::Result;

use crate::script::{context::ThreadState, parser::Parser};
use crate::subsystem::resources::{
    motion_manager::DissolveType,
    thread_manager::ThreadManager,
    thread_wrapper::ThreadRequest,
};
use crate::subsystem::world::GameData;
use crate::debug_ui;

/// Drives the script VM (which is coroutine-based, not OS-thread based).
///
/// Design goal: isolate context-switching and opcode execution from the rest of the engine loop,
/// while keeping the refactor minimally invasive.
#[derive(Debug)]
pub struct VmRunner {
    tm: ThreadManager,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct VmTickReport {
    /// True if at least one context was force-yielded due to opcode budget exhaustion.
    pub forced_yield: bool,
    /// Number of contexts that hit the opcode budget in this tick.
    pub forced_yield_contexts: u32,
}

#[derive(Debug, Default, Clone, Copy)]
struct VmContextReport {
    forced_yield: bool,
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
    pub fn tick(&mut self, game: &mut GameData, parser: &mut Parser, frame_time_ms: u64) -> Result<VmTickReport> {
        // The VM itself is cooperative; the engine decides when to advance contexts.
        // If the game is halted (e.g. waiting for IO / modal UI), we do not advance contexts.
        if game.get_halt() {
            if debug_ui::enabled() {
                game.debug_vm_mut().update_from_thread_manager(&self.tm);
            }
            return Ok(VmTickReport::default());
        }

        // In the original engine, dissolve is a global visual state that can unblock VM waits.
        let dissolve_type = game.motion_manager.get_dissolve_type();
        let dissolve2_transitioning = game.motion_manager.is_dissolve2_transitioning();

        // Hard cap of opcode dispatches per frame to avoid the VM monopolizing the engine loop.
        // This is critical for games that spin in script (polling input, timers, etc.).
        // The original engine is cooperative; we must enforce cooperation even if a syscall
        // forgets to yield.
        let max_ops_per_context: usize = std::env::var("RFVP_VM_MAX_OPS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            // Default is intentionally high: the VM often executes a burst of opcodes
            // between script-requested yield points (WAIT/SLEEP/NEXT/etc.). A too-small
            // budget introduces artificial yields that can expose transient scene states.
            .unwrap_or(20000);

        let mut report = VmTickReport::default();

        let total = self.tm.total_contexts() as u32;
        for tid in 0..total {
            if game.get_halt() {
                break;
            }

            // Legacy behavior: when the game is terminating, only the last-running context keeps advancing.
            if game.get_game_should_exit() && game.get_last_current_thread() != tid {
                continue;
            }

            self.advance_timers_and_state(tid, dissolve_type, dissolve2_transitioning, frame_time_ms);

            let status = self.tm.get_context_status(tid);
            if status.contains(ThreadState::CONTEXT_STATUS_RUNNING)
                && !status.contains(ThreadState::CONTEXT_STATUS_WAIT)
                && !status.contains(ThreadState::CONTEXT_STATUS_SLEEP)
                && !status.contains(ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT)
            {
                let ctx_report = self.run_one_context(tid, game, parser, max_ops_per_context)?;
                if ctx_report.forced_yield {
                    report.forced_yield = true;
                    report.forced_yield_contexts = report.forced_yield_contexts.saturating_add(1);
                }
            }
        }

        // ExitMode(3): once the designated "last current" context has exited, signal the host loop.
        if game.get_game_should_exit() {
            let main_tid = game.get_last_current_thread();
            let st = self.tm.get_context_status(main_tid);
            if st == ThreadState::CONTEXT_STATUS_NONE || self.tm.get_should_break() {
                game.set_main_thread_exited(true);
            }
        }

        if debug_ui::enabled() {
            game.debug_vm_mut().update_from_thread_manager(&self.tm);
        }

        Ok(report)
    }

    fn advance_timers_and_state(&mut self, tid: u32, dissolve_type: DissolveType, dissolve2_transitioning: bool, frame_time_ms: u64) {
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

        // SLEEP timer
        if status.contains(ThreadState::CONTEXT_STATUS_SLEEP) {
            let sleep_time = self.tm.get_context_sleeping_time(tid);
            if sleep_time > frame_time_ms {
                self.tm.set_context_sleeping_time(tid, sleep_time - frame_time_ms);
            } else {
                self.tm.set_context_sleeping_time(tid, 0);
                let mut new_status = status.clone();
                new_status.remove(ThreadState::CONTEXT_STATUS_SLEEP);
                new_status.insert(ThreadState::CONTEXT_STATUS_RUNNING);
                self.tm.set_context_status(tid, new_status);
            }
        }

        // Dissolve wait is unblocked when dissolve is completed / static, and dissolve2 is not transitioning.
        if status.contains(ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT)
            && (dissolve_type == DissolveType::None || dissolve_type == DissolveType::Static)
            && !dissolve2_transitioning
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
    ) -> Result<VmContextReport> {
        // Keep the VM's notion of current thread aligned with GameData.
        self.tm.set_current_id(tid);
        game.set_current_thread(tid);

        self.tm.set_context_should_break(tid, false);
        let mut forced_yield = false;
        while !self.tm.get_context_should_break(tid) {
            if opcode_budget == 0 {
                // Force-yield to keep the engine responsive.
                // Note: this is an artificial yield point (not requested by script). The host loop may
                // want to keep pumping the VM in the same render frame to avoid showing intermediate
                // scene states.
                forced_yield = true;
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

            let mut must_yield = false;

            // Drain all pending requests emitted by syscalls.
            while let Some(event) = game.thread_wrapper.pop() {
                match event {
                    ThreadRequest::Start(id, addr) => self.tm.thread_start(id, addr),
                    ThreadRequest::Wait(time) => {
                        self.tm.thread_wait(time);
                        must_yield = true;
                    },
                    ThreadRequest::DissolveWait() => {
                        self.tm.thread_dissolve_wait();
                        must_yield = true;
                    },
                    ThreadRequest::Sleep(time) => {
                        self.tm.thread_sleep(time);
                        must_yield = true;
                    }
                    ThreadRequest::Raise(time) => {
                        self.tm.thread_raise(time);
                        must_yield = true;
                    }
                    ThreadRequest::Next() => {
                        self.tm.thread_next();
                        must_yield = true;
                    }
                    ThreadRequest::Exit(id) => {
                        self.tm.thread_exit(id);
                        must_yield = true;
                    }
                    ThreadRequest::ShouldBreak() => {
                        // Must break the CURRENT context, not a global flag.
                        self.tm.set_context_should_break(tid, true);
                        self.tm.set_should_break(true);
                        must_yield = true;
                    }
                }
            }

            if must_yield {
                // Force a per-context yield at frame boundary.
                self.tm.set_context_should_break(tid, true);
                break;
            }
        }

        Ok(VmContextReport { forced_yield })
    }
}
