use std::cell::RefCell;
use std::sync::{Arc, RwLock};

use crate::script::parser::Parser;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::subsystem::world::GameData;
use crate::vm_runner::{VmRunner, VmTickReport};

pub struct VmWorker {
    state: RefCell<VmWorkerState>,
}

struct VmWorkerState {
    game_data: Arc<RwLock<Box<GameData>>>,
    parser: Parser,
    vm: VmRunner,
    input_signal_pending: bool,
}

impl VmWorker {
    pub fn spawn(
        game_data: Arc<RwLock<Box<GameData>>>,
        parser: Parser,
        script_engine: ThreadManager,
    ) -> Self {
        Self {
            state: RefCell::new(VmWorkerState {
                game_data,
                parser,
                vm: VmRunner::new(script_engine),
                input_signal_pending: false,
            }),
        }
    }

    #[inline]
    pub fn send_frame_ms(&self, frame_ms: u64) {
        let _ = self.tick(frame_ms);
    }

    #[inline]
    pub fn send_dissolve_done(&self) {
        let _ = self.tick(0);
    }

    #[inline]
    pub fn send_input_signal(&self) {
        let mut state = self.state.borrow_mut();
        if state.input_signal_pending {
            return;
        }
        state.input_signal_pending = true;
        drop(state);
        let _ = self.tick(0);
        self.state.borrow_mut().input_signal_pending = false;
    }

    #[inline]
    pub fn send_frame_ms_sync(&self, frame_ms: u64) -> VmTickReport {
        self.tick(frame_ms)
    }

    #[inline]
    pub fn send_dissolve_done_sync(&self) {
        let _ = self.tick(0);
    }

    fn tick(&self, frame_ms: u64) -> VmTickReport {
        let game_data = self.state.borrow().game_data.clone();
        let mut gd = match game_data.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if gd.get_halt() {
            return VmTickReport::default();
        }

        let mut state = self.state.borrow_mut();
        let VmWorkerState { parser, vm, .. } = &mut *state;
        match vm.tick(&mut **gd, parser, frame_ms) {
            Ok(report) => report,
            Err(e) => {
                log::error!("VM wasm tick error: {e:?}");
                VmTickReport::default()
            }
        }
    }
}
