use anyhow::Result;
use std::fs;

use crate::script::{context::ThreadState, parser::Parser};
use crate::subsystem::resources::{
    motion_manager::DissolveType,
    thread_manager::ThreadManager,
    thread_wrapper::ThreadRequest,
};
use crate::subsystem::world::GameData;
use crate::subsystem::resources::save_manager::SaveItem;
use crate::debug_ui;
use crate::subsystem::save_state::try_decode_state_chunk_v1;

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
    /// Reserved for compatibility with the existing host/worker interface.
    /// With opcode-budget slicing removed, the VM no longer reports artificial forced yields.
    pub forced_yield: bool,
    /// Reserved for compatibility with the existing host/worker interface.
    pub forced_yield_contexts: u32,
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
        self.process_deferred_text_requests(game);
        // The VM itself is cooperative; the engine decides when to advance contexts.
        // If the game is halted (e.g. waiting for IO / modal UI), we do not advance contexts.
        if game.get_halt() {
            if debug_ui::enabled() {
                game.debug_vm_mut().update_from_thread_manager(&self.tm);
            }
            return Ok(VmTickReport::default());
        }

        // If a save capture is pending, snapshot the VM state now (on the VM thread) so the
// render thread can serialize it without accessing the VM internals.
if game.save_manager.wants_vm_snapshot_capture() && !game.save_manager.has_pending_vm_snapshot() {
    let snap = self.tm.capture_snapshot_v1();
    game.save_manager.set_pending_vm_snapshot(snap);
}

// Process deferred load requests at a safe point (between VM ticks).
if let Some(slot) = game.save_manager.take_load_request() {
    let path = SaveItem::resolve_save_path_for_read(slot);
    match fs::read(&path) {
        Ok(bytes) => {
            let nls = game.get_nls();
            if let Err(e) = game.save_manager.load_slot_into_current_from_bytes(slot, nls, &bytes) {
                log::error!("load: failed to parse save header for slot {}: {:#}", slot, e);
            }

            match try_decode_state_chunk_v1(&bytes) {
                Ok(Some(s)) => {
                    if let Err(e) = s.apply(game, &mut self.tm) {
                        log::error!("load: apply SaveStateSnapshotV1 failed: {:#}", e);
                    }
                }
                Ok(None) => {
                    // No RFVS chunk: header-only save (engine save or older rfvp save).
                    log::warn!("load: no RFVS chunk found in slot {} (header-only load)", slot);
                }
                Err(e) => {
                    log::error!("load: failed to decode RFVS chunk for slot {}: {:#}", slot, e);
                }
            }
        }
        Err(e) => {
            log::error!("load: failed to read save slot {} from {}: {:#}", slot, path.display(), e);
        }
    }

    // Do not advance contexts in the same tick; resume on the next frame.
    if debug_ui::enabled() {
        game.debug_vm_mut().update_from_thread_manager(&self.tm);
    }
    return Ok(VmTickReport::default());
}



// In the original engine, dissolve is a global visual state that can unblock VM waits.
        let dissolve_type = game.motion_manager.get_dissolve_type();
        let dissolve2_transitioning = game.motion_manager.is_dissolve2_transitioning();

        let report = VmTickReport::default();

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
                && !status.contains(ThreadState::CONTEXT_STATUS_TEXT)
                && !status.contains(ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT)
            {
                self.run_one_context(tid, game, parser)?;
            }
        }

        // ExitMode(3): once the designated "last current" context has actually exited,
        // signal the host loop. Ordinary should_break/yield must not count as exit.
        if game.get_lock_scripter() {
            let main_tid = game.get_last_current_thread();
            let st = self.tm.get_context_status(main_tid);
            if st == ThreadState::CONTEXT_STATUS_NONE {
                game.set_main_thread_exited(true);
            }
        }


// If a save capture is pending, snapshot VM state after the tick so the render thread
// can serialize a consistent coroutine state for this frame.
if game.save_manager.wants_vm_snapshot_capture() {
    let snap = self.tm.capture_snapshot_v1();
    game.save_manager.set_pending_vm_snapshot(snap);
}

        if debug_ui::enabled() {
            game.debug_vm_mut().update_from_thread_manager(&self.tm);
        }

        Ok(report)
    }

    fn process_deferred_text_requests(&mut self, game: &mut GameData) {
        let mut keep = std::collections::VecDeque::new();
        while let Some(req) = game.thread_wrapper.pop() {
            match req {
                ThreadRequest::TextResume(id) => {
                    let mut st = self.tm.get_context_status(id);
                    st.remove(ThreadState::CONTEXT_STATUS_TEXT);
                    st.insert(ThreadState::CONTEXT_STATUS_RUNNING);
                    self.tm.set_context_status(id, st);
                }
                other => keep.push_back(other),
            }
        }
        while let Some(req) = keep.pop_front() {
            match req {
                ThreadRequest::Start(id, addr) => game.thread_wrapper.thread_start(id, addr),
                ThreadRequest::Wait(time) => game.thread_wrapper.thread_wait(time),
                ThreadRequest::DissolveWait() => game.thread_wrapper.dissolve_wait(),
                ThreadRequest::Sleep(time) => game.thread_wrapper.thread_sleep(time),
                ThreadRequest::Raise(time) => game.thread_wrapper.thread_raise(time),
                ThreadRequest::Next() => game.thread_wrapper.thread_next(),
                ThreadRequest::TextWait(id) => game.thread_wrapper.thread_text_wait(id),
                ThreadRequest::Exit(id) => game.thread_wrapper.thread_exit(id),
                ThreadRequest::ShouldBreak() => game.thread_wrapper.should_break(),
                ThreadRequest::TextResume(id) => game.thread_wrapper.thread_text_resume(id),
            }
        }
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
    ) -> Result<()> {
        // Keep the VM's notion of current thread aligned with GameData.
        self.tm.set_current_id(tid);
        game.set_current_thread(tid);

        self.tm.set_context_should_break(tid, false);
        while !self.tm.get_context_should_break(tid) {
            let result = self.tm.context_dispatch_opcode(tid, game, parser);

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
                    ThreadRequest::TextWait(id) => {
                        let mut st = self.tm.get_context_status(id);
                        st.insert(ThreadState::CONTEXT_STATUS_TEXT);
                        st.remove(ThreadState::CONTEXT_STATUS_RUNNING);
                        self.tm.set_context_status(id, st);
                        must_yield = true;
                    }
                    ThreadRequest::TextResume(id) => {
                        let mut st = self.tm.get_context_status(id);
                        st.remove(ThreadState::CONTEXT_STATUS_TEXT);
                        st.insert(ThreadState::CONTEXT_STATUS_RUNNING);
                        self.tm.set_context_status(id, st);
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

        Ok(())
    }
}
