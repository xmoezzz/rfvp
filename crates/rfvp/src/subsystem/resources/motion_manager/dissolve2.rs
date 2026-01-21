/// Second dissolve system (engine-internal overlay fade).
///
/// This is intentionally separate from the script-controlled dissolve (mask/colored dissolve1).
/// The original engine uses a small mode machine:
///   0: off
///   1: hold (opaque)
///   2: fade-out (opaque -> transparent)
///   3: fade-in  (transparent -> opaque)
///
/// This module provides a small state machine with time-based alpha update.
#[derive(Debug, Clone)]
pub(crate) struct Dissolve2State {
    mode: u8,
    color_id: u32,
    duration_ms: u32,
    elapsed_ms: u32,
    alpha: f32,

    /// When true, after fade-in reaches opaque, automatically switch to fade-out
    /// on the next tick (used for load/save "black then reveal").
    pending_fade_out: bool,
}

impl Dissolve2State {
    pub(crate) fn new() -> Self {
        Self {
            mode: 0,
            color_id: 0,
            duration_ms: 1,
            elapsed_ms: 0,
            alpha: 0.0,
            pending_fade_out: false,
        }
    }

    pub(crate) fn mode(&self) -> u8 {
        self.mode
    }

    pub(crate) fn color_id(&self) -> u32 {
        self.color_id
    }

    pub(crate) fn alpha(&self) -> f32 {
        self.alpha
    }

    pub(crate) fn is_transitioning(&self) -> bool {
        self.mode == 2 || self.mode == 3
    }

    pub(crate) fn is_active(&self) -> bool {
        self.mode != 0
    }

    pub(crate) fn reset(&mut self) {
        self.mode = 0;
        self.color_id = 0;
        self.duration_ms = 1;
        self.elapsed_ms = 0;
        self.alpha = 0.0;
        self.pending_fade_out = false;
    }

    pub(crate) fn start_hold(&mut self, color_id: u32) {
        self.mode = 1;
        self.color_id = color_id;
        self.duration_ms = 1;
        self.elapsed_ms = 0;
        self.alpha = 1.0;
        self.pending_fade_out = false;
    }

    pub(crate) fn start_fade_in(&mut self, color_id: u32, duration_ms: u32) {
        self.mode = 3;
        self.color_id = color_id;
        self.duration_ms = duration_ms.max(1);
        self.elapsed_ms = 0;
        self.alpha = 0.0;
        self.pending_fade_out = false;
    }

    pub(crate) fn start_fade_out(&mut self, duration_ms: u32) {
        // Fade-out keeps previous color_id (expected behavior for load/save).
        self.mode = 2;
        self.duration_ms = duration_ms.max(1);
        self.elapsed_ms = 0;
        self.alpha = 1.0;
        self.pending_fade_out = false;
    }

    /// Convenience for "fade to opaque then reveal", matching the original load-style usage.
    pub(crate) fn start_in_out(&mut self, color_id: u32, duration_ms: u32) {
        self.start_fade_in(color_id, duration_ms);
        self.pending_fade_out = true;
    }

    /// Advance the state by elapsed time.
    ///
    /// If `elapsed_ms` is `u32::MAX`, we treat it as fast-forward and finish the transition
    /// immediately (used by Ctrl/ControlPulse semantics in the engine).
    pub(crate) fn tick(&mut self, elapsed_ms: u32) {
        if self.mode == 0 {
            self.alpha = 0.0;
            self.pending_fade_out = false;
            return;
        }

        // Hold: stay opaque until externally changed.
        if self.mode == 1 {
            self.alpha = 1.0;
            self.pending_fade_out = false;
            return;
        }

        // Transitioning: 2 (fade-out) or 3 (fade-in)
        if elapsed_ms == u32::MAX {
            self.elapsed_ms = self.duration_ms;
        } else {
            self.elapsed_ms = self.elapsed_ms.saturating_add(elapsed_ms);
        }

        let dur = self.duration_ms.max(1);
        let t = (self.elapsed_ms as f32 / dur as f32).clamp(0.0, 1.0);

        match self.mode {
            3 => {
                // fade-in: transparent -> opaque
                self.alpha = t;
                if self.elapsed_ms >= dur {
                    self.alpha = 1.0;
                    if self.pending_fade_out {
                        // Switch to fade-out after reaching opaque.
                        self.pending_fade_out = false;
                        self.mode = 2;
                        self.elapsed_ms = 0;
                        // keep alpha at 1.0 for the first frame of fade-out
                        self.alpha = 1.0;
                    } else {
                        // become hold
                        self.mode = 1;
                        self.alpha = 1.0;
                    }
                }
            }
            2 => {
                // fade-out: opaque -> transparent
                self.alpha = 1.0 - t;
                if self.elapsed_ms >= dur {
                    self.mode = 0;
                    self.alpha = 0.0;
                    self.pending_fade_out = false;
                }
            }
            _ => {
                // Defensive: unknown mode -> off
                self.mode = 0;
                self.alpha = 0.0;
                self.pending_fade_out = false;
            }
        }
    }
}

impl Default for Dissolve2State {
    fn default() -> Self {
        Self::new()
    }
}
