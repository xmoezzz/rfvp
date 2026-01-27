use anyhow::Result;

use crate::subsystem::resources::prim::{PrimManager, INVAILD_PRIM_HANDLE};

fn should_skip_update(prim_manager: &PrimManager, prim_id: u32, flag: bool) -> bool {
    let custom_root_id = prim_manager.get_custom_root_prim_id();

    if flag {
        // Custom-root traversal gate.
        if custom_root_id == 0 {
            return true;
        }
        // Require the primitive to have a parent chain reaching the custom root.
        let mut cur: i16 = prim_id as i16;
        let parent0 = prim_manager.get_prim_immutable(cur).get_parent();
        if parent0 == INVAILD_PRIM_HANDLE {
            return true;
        }
        let mut next = parent0;
        while next != INVAILD_PRIM_HANDLE {
            if next as u16 == custom_root_id {
                return false;
            }
            next = prim_manager.get_prim_immutable(next).get_parent();
        }
        return true;
    }

    // Pause gate: if self or any ancestor is paused, skip updating but keep motion running.
    let mut cur: i16 = prim_id as i16;
    loop {
        let p = prim_manager.get_prim_immutable(cur);
        if p.get_paused() {
            return true;
        }
        let parent = p.get_parent();
        drop(p);
        if parent == INVAILD_PRIM_HANDLE {
            break;
        }
        cur = parent;
    }

    false
}

#[derive(Debug, Clone, PartialEq)]
pub enum MoveMotionType {
    None = 0,
    Linear,
    Accelerate,
    Decelerate,
    Rebound,
    Bounce,
}

impl TryFrom<i32> for MoveMotionType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MoveMotionType::None),
            1 => Ok(MoveMotionType::Linear),
            2 => Ok(MoveMotionType::Accelerate),
            3 => Ok(MoveMotionType::Decelerate),
            4 => Ok(MoveMotionType::Rebound),
            5 => Ok(MoveMotionType::Bounce),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MoveMotion {
    id: u16,
    prim_id: u32,
    running: bool,
    reverse: bool,
    src_x: i32,
    src_y: i32,
    dst_x: i32,
    dst_y: i32,
    duration: i32,
    elapsed: i32,
    anm_type: MoveMotionType,
}

impl MoveMotion {
    pub fn new(id: u16) -> MoveMotion {
        MoveMotion {
            id,
            prim_id: 0,
            running: false,
            reverse: false,
            src_x: 0,
            src_y: 0,
            dst_x: 0,
            dst_y: 0,
            duration: 0,
            elapsed: 0,
            anm_type: MoveMotionType::None,
        }
    }

    fn finish(&mut self) {
        self.running = false;
        self.anm_type = MoveMotionType::None;
        self.prim_id = 0;
        self.elapsed = 0;
        self.duration = 0;
    }

    pub fn is_running_for(&self, prim_id: u32) -> bool {
        self.running && self.prim_id == prim_id && self.anm_type != MoveMotionType::None
    }

    pub fn update(&mut self, prim_manager: &PrimManager, flag: bool, elapsed_delta: i32) -> bool {
        if self.anm_type == MoveMotionType::None || self.prim_id as i16 == INVAILD_PRIM_HANDLE {
            return true;
        }

        if should_skip_update(prim_manager, self.prim_id, flag) {
            return true;
        }

        let mut dt = elapsed_delta;
        if self.reverse && dt < 0 {
            dt = -dt;
        }

        self.elapsed += dt;

        // Commit final state on negative tick or completion.
        if dt < 0 || self.elapsed >= self.duration {
            let mut prim = prim_manager.get_prim(self.prim_id as i16);
            prim.apply_attr(0x40);
            prim.set_x(self.dst_x as i16);
            prim.set_y(self.dst_y as i16);
            return false;
        }

        let src_x = self.src_x as i64;
        let src_y = self.src_y as i64;
        let dst_x = self.dst_x as i64;
        let dst_y = self.dst_y as i64;
        let delta_x = dst_x - src_x;
        let delta_y = dst_y - src_y;
        let e = self.elapsed as i64;
        let d = self.duration as i64;

        let (x, y) = match self.anm_type {
            MoveMotionType::Linear => (src_x + delta_x * e / d, src_y + delta_y * e / d),
            MoveMotionType::Accelerate => (
                src_x + delta_x * e * e / (d * d),
                src_y + delta_y * e * e / (d * d),
            ),
            MoveMotionType::Decelerate => (
                dst_x - delta_x * (d - e) * (d - e) / (d * d),
                dst_y - delta_y * (d - e) * (d - e) / (d * d),
            ),
            MoveMotionType::Rebound => {
                let half_delta_x = delta_x / 2;
                let half_delta_y = delta_y / 2;
                let half_dur = d / 2;
                if e > half_dur {
                    let denom = (d - half_dur) * (d - half_dur);
                    let t = (d - e) * (d - e);
                    (
                        dst_x - (delta_x - half_delta_x) * t / denom,
                        dst_y - (delta_y - half_delta_y) * t / denom,
                    )
                } else {
                    let denom = half_dur * half_dur;
                    (
                        src_x + half_delta_x * e * e / denom,
                        src_y + half_delta_y * e * e / denom,
                    )
                }
            }
            MoveMotionType::Bounce => {
                let half_delta_x = delta_x / 2;
                let half_delta_y = delta_y / 2;
                let half_dur = d / 2;
                if e > half_dur {
                    let t = (e - half_dur) * (e - half_dur);
                    let denom = (d - half_dur) * (d - half_dur);
                    (
                        half_delta_x + src_x + (delta_x - half_delta_x) * t / denom,
                        half_delta_y + src_y + (delta_y - half_delta_y) * t / denom,
                    )
                } else {
                    let t = (half_dur - e) * (half_dur - e);
                    let denom = half_dur * half_dur;
                    (
                        half_delta_x + src_x - half_delta_x * t / denom,
                        half_delta_y + src_y - half_delta_y * t / denom,
                    )
                }
            }
            _ => (dst_x, dst_y),
        };

        let mut prim = prim_manager.get_prim(self.prim_id as i16);
        prim.apply_attr(0x40);
        prim.set_x(x as i16);
        prim.set_y(y as i16);
        true
    }
}

pub struct MoveMotionContainer {
    motions: Vec<MoveMotion>,
    free_ids: Vec<u16>,
}

impl MoveMotionContainer {
    pub fn new() -> MoveMotionContainer {
        let mut motions = Vec::with_capacity(512);
        for i in 0..512u16 {
            motions.push(MoveMotion::new(i));
        }
        let free_ids: Vec<u16> = (0..512u16).rev().collect();
        MoveMotionContainer { motions, free_ids }
    }

    pub fn push_motion(
        &mut self,
        prim_id: u32,
        src_x: i32,
        src_y: i32,
        dst_x: i32,
        dst_y: i32,
        duration: i32,
        anm_type: MoveMotionType,
        reverse: bool,
    ) -> Result<()> {
        // Only one motion per prim.
        let _ = self.stop_motion(prim_id);

        let Some(id) = self.free_ids.pop() else {
            return Ok(());
        };

        let m = &mut self.motions[id as usize];
        m.prim_id = prim_id;
        m.running = true;
        m.reverse = reverse;
        m.src_x = src_x;
        m.src_y = src_y;
        m.dst_x = dst_x;
        m.dst_y = dst_y;
        m.duration = duration;
        m.elapsed = 0;
        m.anm_type = anm_type;
        Ok(())
    }

    pub fn stop_motion(&mut self, prim_id: u32) -> Result<()> {
        for m in &mut self.motions {
            if m.is_running_for(prim_id) {
                let id = m.id;
                m.finish();
                self.free_ids.push(id);
                break;
            }
        }
        Ok(())
    }

    pub fn test_motion(&self, prim_id: u32) -> bool {
        self.motions.iter().any(|m| m.is_running_for(prim_id))
    }

    pub fn exec_move_motion(&mut self, prim_manager: &PrimManager, flag: bool, elapsed: i32) {
        for i in 0..self.motions.len() {
            if !self.motions[i].running {
                continue;
            }
            if !self.motions[i].update(prim_manager, flag, elapsed) {
                let id = self.motions[i].id;
                self.motions[i].finish();
                self.free_ids.push(id);
            }
        }
    }

    pub fn debug_dump(&self, max: usize) -> String {
        let mut s = String::new();
        let mut count = 0;
        for m in &self.motions {
            if m.running {
                s.push_str(&format!(
                    "Motion ID: {}, Prim ID: {}, Src: ({}, {}), Dst: ({}, {}), Elapsed: {}, Duration: {}, Type: {:?}, Reverse: {}\n",
                    m.id, m.prim_id, m.src_x, m.src_y, m.dst_x, m.dst_y, m.elapsed, m.duration, m.anm_type, m.reverse
                ));
                count += 1;
                if count >= max {
                    break;
                }
            }
        }
        s
    }

    pub fn debug_running_count(&self) -> usize {
        self.motions.iter().filter(|m| m.running).count()
    }
}
