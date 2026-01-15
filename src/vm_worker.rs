
use std::sync::{Arc, RwLock};
use std::sync::mpsc::{self, Sender};
use std::thread;

use crate::script::parser::Parser;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::subsystem::world::GameData;
use crate::vm_runner::VmRunner;

/// Events delivered from the main thread to the VM thread.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// Advance the script VM by one frame worth of time.
    Frame { frame_ms: u64 },

    /// Notify the VM that a global dissolve has finished.
    ///
    /// This is used to unblock contexts waiting on DISSOLVE_WAIT without
    /// having to wait for the next frame event. The VM will run a zero-delta
    /// tick to process state transitions.
    DissolveDone,

    /// Notify the VM that an input event was received on the main thread.
    ///
    /// We do not carry payload here: the main thread already recorded the
    /// input into `GameData.inputs_manager` while holding the same write lock.
    /// This event exists to wake the VM so scripts can react immediately
    /// without waiting for the next frame.
    InputSignal,

    /// Terminate the VM thread.
    Stop,
}

/// A lightweight VM worker that runs the coroutine-based VM on a dedicated OS thread.
///
/// Design constraints:
/// - The VM thread must never touch wgpu objects (Device/Queue/Surface/Textures).
/// - All state mutations happen under a shared RwLock<GameData>.
///   The main thread should send Frame events from `AboutToWait` so the VM runs
///   while the event loop is idle and does not block rendering.
pub struct VmWorker {
    tx: Sender<EngineEvent>,
}

impl VmWorker {
    pub fn spawn(
        game_data: Arc<RwLock<GameData>>,
        mut parser: Parser,
        script_engine: ThreadManager,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<EngineEvent>();

        thread::Builder::new()
            .name("rfvp-vm".to_string())
            .spawn(move || {
                let mut vm = VmRunner::new(script_engine);

                while let Ok(ev) = rx.recv() {
                    match ev {
                        EngineEvent::Stop => break,
                        EngineEvent::Frame { frame_ms } => {
                            // Tick the VM with a short critical section.
                            // The main thread should avoid holding the write lock during AboutToWait.
                            let mut gd = match game_data.write() {
                                Ok(g) => g,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            if gd.get_halt() {
                                // If the VM asked to halt, do not progress this frame.
                                continue;
                            }
                            if let Err(e) = vm.tick(&mut gd, &mut parser, frame_ms) {
                                log::error!("VM thread tick error: {e:?}");
                                // Keep running; scripts may recover depending on engine behavior.
                            }
                        }
                        EngineEvent::DissolveDone => {
                            let mut gd = match game_data.write() {
                                Ok(g) => g,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            if gd.get_halt() {
                                continue;
                            }
                            // Zero-delta tick: only advances internal state (e.g., clears DISSOLVE_WAIT)
                            // based on already-updated global motion state.
                            if let Err(e) = vm.tick(&mut gd, &mut parser, 0) {
                                log::error!("VM thread tick error (dissolve done): {e:?}");
                            }
                        }
                        EngineEvent::InputSignal => {
                            let mut gd = match game_data.write() {
                                Ok(g) => g,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            if gd.get_halt() {
                                continue;
                            }
                            // Zero-delta tick: lets scripts observe the already-updated
                            // input state immediately (e.g., polling InputGetEvent).
                            if let Err(e) = vm.tick(&mut gd, &mut parser, 0) {
                                log::error!("VM thread tick error (input signal): {e:?}");
                            }
                        }
                    }
                }
            })
            .expect("failed to spawn VM thread");

        Self { tx }
    }

    #[inline]
    pub fn send_frame_ms(&self, frame_ms: u64) {
        // Ignore send errors during shutdown.
        let _ = self.tx.send(EngineEvent::Frame { frame_ms });
    }

    #[inline]
    pub fn send_dissolve_done(&self) {
        let _ = self.tx.send(EngineEvent::DissolveDone);
    }

    #[inline]
    pub fn send_input_signal(&self) {
        let _ = self.tx.send(EngineEvent::InputSignal);
    }
}
