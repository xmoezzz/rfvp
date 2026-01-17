use crate::script::context::ThreadState;
use crate::subsystem::resources::thread_manager::ThreadManager;

#[derive(Debug, Clone, Default)]
pub struct ContextEntry {
    pub id: u32,
    pub status_bits: u32,
    pub wait_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct VmSnapshot {
    pub tick_seq: u64,
    pub current_id: u32,
    pub entries: Vec<ContextEntry>,
}

impl VmSnapshot {
    pub fn summarize_counts(&self) -> (usize, usize, usize, usize) {
        let mut run = 0usize;
        let mut wait = 0usize;
        let mut sleep = 0usize;
        let mut dissolve_wait = 0usize;
        for e in &self.entries {
            let bits = e.status_bits;
            if (bits & ThreadState::CONTEXT_STATUS_RUNNING.bits()) != 0 {
                run += 1;
            }
            if (bits & ThreadState::CONTEXT_STATUS_WAIT.bits()) != 0 {
                wait += 1;
            }
            if (bits & ThreadState::CONTEXT_STATUS_SLEEP.bits()) != 0 {
                sleep += 1;
            }
            if (bits & ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT.bits()) != 0 {
                dissolve_wait += 1;
            }
        }
        (run, wait, sleep, dissolve_wait)
    }
}

impl VmSnapshot {
    pub fn update_from_thread_manager(&mut self, tm: &ThreadManager) {
        self.current_id = tm.get_current_id();
        self.entries.clear();
        self.entries.reserve(tm.total_contexts());

        for id in 0..(tm.total_contexts() as u32) {
            let st: ThreadState = tm.get_context_status(id);
            let wait = tm.get_context_waiting_time(id);
            self.entries.push(ContextEntry {
                id,
                status_bits: st.bits(),
                wait_ms: wait,
            });
        }

        self.tick_seq = self.tick_seq.wrapping_add(1);
    }

}
