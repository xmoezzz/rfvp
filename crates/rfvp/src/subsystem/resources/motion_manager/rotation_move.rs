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
pub enum RotationMotionType {
    None = 0,
    Linear,
    Accelerate,
    Decelerate,
    Rebound,
    Bounce,
}

impl TryFrom<i32> for RotationMotionType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(RotationMotionType::None),
            1 => Ok(RotationMotionType::Linear),
            2 => Ok(RotationMotionType::Accelerate),
            3 => Ok(RotationMotionType::Decelerate),
            4 => Ok(RotationMotionType::Rebound),
            5 => Ok(RotationMotionType::Bounce),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RotationMotion {
    id: u16,
    prim_id: u32,
    running: bool,
    reverse: bool,
    src_angle: i16,
    dst_angle: i16,
    duration: i32,
    elapsed: i32,
    typ: RotationMotionType,
}

impl RotationMotion {
    pub fn new(id: u16) -> RotationMotion {
        RotationMotion {
            id,
            prim_id: 0,
            running: false,
            reverse: false,
            src_angle: 0,
            dst_angle: 0,
            duration: 0,
            elapsed: 0,
            typ: RotationMotionType::None,
        }
    }

    fn finish(&mut self) {
        self.running = false;
        self.typ = RotationMotionType::None;
        self.prim_id = 0;
        self.elapsed = 0;
        self.duration = 0;
    }

    pub fn is_running_for(&self, prim_id: u32) -> bool {
        self.running && self.prim_id == prim_id && self.typ != RotationMotionType::None
    }

    pub fn update(&mut self, prim_manager: &PrimManager, flag: bool, elapsed_delta: i32) -> bool {
        if self.typ == RotationMotionType::None || self.prim_id as i16 == INVAILD_PRIM_HANDLE {
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

        if dt < 0 || self.elapsed >= self.duration {
            let mut prim = prim_manager.get_prim(self.prim_id as i16);
            prim.apply_attr(0x40);
            prim.set_rotation(self.dst_angle);
            return false;
        }

        let src = self.src_angle as i64;
        let dst = self.dst_angle as i64;
        let delta = dst - src;
        let e = self.elapsed as i64;
        let d = self.duration as i64;

        let r = match self.typ {
            RotationMotionType::Linear => src + delta * e / d,
            RotationMotionType::Accelerate => src + delta * e * e / (d * d),
            RotationMotionType::Decelerate => dst - delta * (d - e) * (d - e) / (d * d),
            RotationMotionType::Rebound => {
                let half_delta = delta / 2;
                let half_dur = d / 2;
                if e > half_dur {
                    let denom = (d - half_dur) * (d - half_dur);
                    let t = (d - e) * (d - e);
                    dst - (delta - half_delta) * t / denom
                } else {
                    let denom = half_dur * half_dur;
                    src + half_delta * e * e / denom
                }
            }
            RotationMotionType::Bounce => {
                let half_delta = delta / 2;
                let half_dur = d / 2;
                if e > half_dur {
                    let t = (e - half_dur) * (e - half_dur);
                    let denom = (d - half_dur) * (d - half_dur);
                    half_delta + src + (delta - half_delta) * t / denom
                } else {
                    let t = (half_dur - e) * (half_dur - e);
                    let denom = half_dur * half_dur;
                    half_delta + src - half_delta * t / denom
                }
            }
            _ => dst,
        };

        let mut prim = prim_manager.get_prim(self.prim_id as i16);
        prim.apply_attr(0x40);
        prim.set_rotation(r as i16);
        true
    }
}

pub struct RotationMotionContainer {
    motions: Vec<RotationMotion>,
    free_ids: Vec<u16>,
}

impl RotationMotionContainer {
    pub fn new() -> RotationMotionContainer {
        let mut motions = Vec::with_capacity(512);
        for i in 0..512u16 {
            motions.push(RotationMotion::new(i));
        }
        let free_ids: Vec<u16> = (0..512u16).rev().collect();
        RotationMotionContainer { motions, free_ids }
    }

    pub fn push_motion(
        &mut self,
        prim_id: u32,
        src_angle: i16,
        dest_angle: i16,
        duration: i32,
        typ: RotationMotionType,
        reverse: bool,
    ) -> Result<()> {
        let _ = self.stop_motion(prim_id);
        let Some(id) = self.free_ids.pop() else { return Ok(()); };
        let m = &mut self.motions[id as usize];
        m.prim_id = prim_id;
        m.running = true;
        m.reverse = reverse;
        m.src_angle = src_angle;
        m.dst_angle = dest_angle;
        m.duration = duration;
        m.elapsed = 0;
        m.typ = typ;
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

    pub fn exec_rotation_motion(&mut self, prim_manager: &PrimManager, flag: bool, elapsed: i32) {
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
        let mut out = String::new();
        let mut n = 0usize;
        for m in &self.motions {
            if !m.running {
                continue;
            }
            if n >= max {
                break;
            }
            out.push_str(&format!(
                "  [rot] prim={} src={} dst={} elapsed={} dur={} type={:?} rev={}
",
                m.prim_id, m.src_angle, m.dst_angle, m.elapsed, m.duration, m.typ, m.reverse
            ));
            n += 1;
        }
        out
    }

    pub fn debug_running_count(&self) -> usize {
        self.motions.iter().filter(|m| m.running).count()
    }
}
