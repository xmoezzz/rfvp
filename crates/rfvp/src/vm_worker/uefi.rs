use std::sync::{Arc, RwLock};

use crate::script::parser::Parser;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::subsystem::world::GameData;
use crate::vm_runner::{VmRunner, VmTickReport};

const UEFI_VERBOSE_VM_TICK_LOGS: bool = false;

/// UEFI VM worker.
///
/// UEFI firmware environments do not provide the normal host threading model
/// used by the desktop worker, so script execution is advanced synchronously
/// from the host frame loop.
pub struct VmWorker {
    game_data: Arc<RwLock<Box<GameData>>>,
    parser: Parser,
    vm: VmRunner,
}

impl VmWorker {
    pub fn spawn(
        game_data: Arc<RwLock<Box<GameData>>>,
        parser: Parser,
        script_engine: ThreadManager,
    ) -> Self {
        Self {
            game_data,
            parser,
            vm: VmRunner::new(script_engine),
        }
    }

    #[inline]
    pub fn send_frame_ms(&mut self, frame_ms: u64) {
        let _ = self.tick(frame_ms);
    }

    #[inline]
    pub fn send_dissolve_done(&mut self) {
        let _ = self.tick(0);
    }

    #[inline]
    pub fn send_input_signal(&mut self) {
        let _ = self.tick(0);
    }

    #[inline]
    pub fn send_frame_ms_sync(&mut self, frame_ms: u64) -> VmTickReport {
        self.tick(frame_ms)
    }

    #[inline]
    pub fn send_dissolve_done_sync(&mut self) {
        let _ = self.tick(0);
    }

    fn tick(&mut self, frame_ms: u64) -> VmTickReport {
        if UEFI_VERBOSE_VM_TICK_LOGS {
            log::info!("[UEFI] VmWorker tick before game_data write lock");
        }
        let mut gd = match self.game_data.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if UEFI_VERBOSE_VM_TICK_LOGS {
            log::info!("[UEFI] VmWorker tick after game_data write lock");
        }

        if gd.get_halt() {
            if UEFI_VERBOSE_VM_TICK_LOGS {
                log::info!("[UEFI] VmWorker tick halted");
            }
            return VmTickReport::default();
        }

        if UEFI_VERBOSE_VM_TICK_LOGS {
            log::info!("[UEFI] VmWorker tick before vm.tick frame_ms={}", frame_ms);
        }
        match self.vm.tick(&mut **gd, &mut self.parser, frame_ms) {
            Ok(report) => {
                if UEFI_VERBOSE_VM_TICK_LOGS {
                    log::info!("[UEFI] VmWorker tick after vm.tick");
                }
                report
            }
            Err(e) => {
                log::error!("UEFI VM tick error: {e:?}");
                VmTickReport::default()
            }
        }
    }
}
