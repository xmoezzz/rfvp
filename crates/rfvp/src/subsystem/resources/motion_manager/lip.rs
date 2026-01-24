use anyhow::Result;
use crate::subsystem::resources::prim::PrimManager;

/// A small lip-animation state machine.
///
/// The original engine ties lip animation to a BGM slot. When the referenced BGM slot is
/// playing, the target sprite primitive cycles through 4 texture frames with per-frame durations.
/// When the BGM stops, the sprite is reset to its first frame.
const LIP_SLOTS: usize = 4;
const LIP_FRAMES: usize = 4;

#[derive(Debug, Clone, Copy)]
struct Frame {
    graph_id: i32,
    dur_ms: u32,
}

impl Default for Frame {
    fn default() -> Self {
        Self { graph_id: 0, dur_ms: 0 }
    }
}

#[derive(Debug, Clone, Copy)]
struct LipSlot {
    // Target primitive id.
    prim_id: i16,
    // BGM slot id (0..3).
    bgm_slot: i32,
    // 0: free, 1: normal (configured), 2: lipsync active.
    kind: u8,
    frames: [Frame; LIP_FRAMES],
    cur_frame: u8,
    acc_ms: u32,
    running: u8,
}

impl Default for LipSlot {
    fn default() -> Self {
        Self {
            prim_id: -1,
            bgm_slot: 0,
            kind: 0,
            frames: [Frame::default(); LIP_FRAMES],
            cur_frame: 0,
            acc_ms: 0,
            running: 0,
        }
    }
}

pub struct LipMotionContainer {
    slots: [LipSlot; LIP_SLOTS],
}

impl LipMotionContainer {
    pub fn new() -> Self {
        Self { slots: [LipSlot::default(); LIP_SLOTS] }
    }

    fn find_or_alloc_slot(&mut self, prim_id: i16) -> Option<usize> {
        if prim_id < 0 {
            return None;
        }
        for (i, s) in self.slots.iter().enumerate() {
            if s.prim_id == prim_id {
                return Some(i);
            }
        }
        for (i, s) in self.slots.iter().enumerate() {
            if s.prim_id < 0 || s.kind == 0 {
                return Some(i);
            }
        }
        None
    }

    pub fn stop_for_prim(&mut self, prim_id: i16) {
        for s in &mut self.slots {
            if s.prim_id == prim_id {
                *s = LipSlot::default();
            }
        }
    }

    /// Configure lip animation frames for a primitive.
    ///
    /// This matches the observed original behavior:
    ///   frames = [id2, id3, id3, id4? id2]
    ///   durs   = [t2,  t3,  t3,  t4]
    pub fn set_motion(
        &mut self,
        prim_id: i16,
        bgm_slot: i32,
        id2: i32,
        t2: u32,
        id3: i32,
        t3: u32,
        id4: i32,
        t4: u32,
    ) -> Result<()> {
        let Some(idx) = self.find_or_alloc_slot(prim_id) else { return Ok(()); };
        let mut s = LipSlot::default();
        s.prim_id = prim_id;
        s.bgm_slot = bgm_slot;
        s.kind = 1;

        let id4 = if id4 > 0 { id4 } else { id2 };
        s.frames[0] = Frame { graph_id: id2, dur_ms: t2 };
        s.frames[1] = Frame { graph_id: id3, dur_ms: t3 };
        s.frames[2] = Frame { graph_id: id3, dur_ms: t3 };
        s.frames[3] = Frame { graph_id: id4, dur_ms: t4 };
        self.slots[idx] = s;
        Ok(())
    }

    /// Enable/disable lipsync.
    pub fn set_lipsync(&mut self, prim_id: i16, enable: bool) {
        for s in &mut self.slots {
            if s.prim_id == prim_id {
                s.kind = if enable { 2 } else { 1 };
                s.cur_frame = 0;
                s.acc_ms = 0;
                s.running = 0;
                return;
            }
        }
    }

    pub fn tick(&mut self, prims: &mut PrimManager, bgm_playing_slots: &[bool], elapsed_ms: i32, freeze: bool) {
        if freeze {
            return;
        }
        if elapsed_ms == 0 {
            return;
        }

        if elapsed_ms < 0 {
            // Fast-forward: reset all active lipsync slots to the first frame.
            for s in &mut self.slots {
                if s.prim_id < 0 || s.kind == 0 {
                    continue;
                }
                s.cur_frame = 0;
                s.acc_ms = 0;
                s.running = 0;
                let gid = s.frames[0].graph_id;
                prims.prim_set_texture_id(s.prim_id as i32, gid);
            }
            return;
        }

        let dt = elapsed_ms as u32;
        for s in &mut self.slots {
            if s.prim_id < 0 || s.kind != 2 {
                continue;
            }

            // When the referenced BGM slot is not playing, reset to first frame.
            let is_playing = bgm_playing_slots.get(s.bgm_slot as usize).copied().unwrap_or(false);
            if !is_playing {
                if s.running != 0 {
                    s.running = 0;
                    s.cur_frame = 0;
                    s.acc_ms = 0;
                    prims.prim_set_texture_id(s.prim_id as i32, s.frames[0].graph_id);
                }
                continue;
            }

            // Start running once BGM becomes active.
            if s.running == 0 {
                s.running = 1;
                s.cur_frame = 0;
                s.acc_ms = 0;
                prims.prim_set_texture_id(s.prim_id as i32, s.frames[0].graph_id);
            }

            s.acc_ms = s.acc_ms.saturating_add(dt);
            // Advance frames while enough time has accumulated.
            for _ in 0..8 {
                let f = s.frames[s.cur_frame as usize];
                let dur = f.dur_ms.max(1);
                if s.acc_ms < dur {
                    break;
                }
                s.acc_ms -= dur;
                s.cur_frame = (s.cur_frame + 1) & 3;
                let next = s.frames[s.cur_frame as usize];
                prims.prim_set_texture_id(s.prim_id as i32, next.graph_id);
            }
        }
    }
}
