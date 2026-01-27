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
pub enum ZMotionType {
    None = 0,
    Linear,
    Accelerate,
    Decelerate,
    Rebound,
    Bounce,
}

impl TryFrom<i32> for ZMotionType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ZMotionType::None),
            1 => Ok(ZMotionType::Linear),
            2 => Ok(ZMotionType::Accelerate),
            3 => Ok(ZMotionType::Decelerate),
            4 => Ok(ZMotionType::Rebound),
            5 => Ok(ZMotionType::Bounce),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ZMotion {
    id: u16,
    prim_id: u32,
    running: bool,
    reverse: bool,
    src_z: i16,
    dst_z: i16,
    duration: i32,
    elapsed: i32,
    typ: ZMotionType,
}

impl ZMotion {
    pub fn new(id: u16) -> ZMotion {
        ZMotion {
            id,
            prim_id: 0,
            running: false,
            reverse: false,
            src_z: 0,
            dst_z: 0,
            duration: 0,
            elapsed: 0,
            typ: ZMotionType::None,
        }
    }

    fn finish(&mut self) {
        self.running = false;
        self.typ = ZMotionType::None;
        self.prim_id = 0;
        self.elapsed = 0;
        self.duration = 0;
    }

    pub fn is_running_for(&self, prim_id: u32) -> bool {
        self.running && self.prim_id == prim_id && self.typ != ZMotionType::None
    }

    pub fn update(&mut self, prim_manager: &PrimManager, flag: bool, elapsed_delta: i32) -> bool {
        if self.typ == ZMotionType::None || self.prim_id as i16 == INVAILD_PRIM_HANDLE {
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
            prim.set_z(self.dst_z);
            return false;
        }

        let src = self.src_z as i64;
        let dst = self.dst_z as i64;
        let delta = dst - src;
        let e = self.elapsed as i64;
        let d = self.duration as i64;

        let z = match self.typ {
            ZMotionType::Linear => src + delta * e / d,
            ZMotionType::Accelerate => src + delta * e * e / (d * d),
            ZMotionType::Decelerate => dst - delta * (d - e) * (d - e) / (d * d),
            ZMotionType::Rebound => {
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
            ZMotionType::Bounce => {
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
        prim.set_z(z as i16);
        true
    }
}

pub struct ZMotionContainer {
    motions: Vec<ZMotion>,
    free_ids: Vec<u16>,
}

impl ZMotionContainer {
    pub fn new() -> ZMotionContainer {
        let mut motions = Vec::with_capacity(512);
        for i in 0..512u16 {
            motions.push(ZMotion::new(i));
        }
        let free_ids: Vec<u16> = (0..512u16).rev().collect();
        ZMotionContainer { motions, free_ids }
    }

    pub fn push_motion(
        &mut self,
        prim_id: u32,
        src_z: i16,
        dst_z: i16,
        duration: i32,
        typ: ZMotionType,
        reverse: bool,
    ) -> Result<()> {
        let _ = self.stop_motion(prim_id);
        let Some(id) = self.free_ids.pop() else { return Ok(()); };
        let m = &mut self.motions[id as usize];
        m.prim_id = prim_id;
        m.running = true;
        m.reverse = reverse;
        m.src_z = src_z;
        m.dst_z = dst_z;
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

    pub fn exec_z_motion(&mut self, prim_manager: &PrimManager, flag: bool, elapsed: i32) {
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
                    "ZMotion id={} prim_id={} src_z={} dst_z={} elapsed={} duration={} type={:?} reverse={}\n",
                    m.id, m.prim_id, m.src_z, m.dst_z, m.elapsed, m.duration, m.typ, m.reverse
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
