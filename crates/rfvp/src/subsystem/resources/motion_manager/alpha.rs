use anyhow::Result;
use crate::subsystem::resources::prim::{PrimManager, INVAILD_PRIM_HANDLE};


#[derive(Debug, Clone, PartialEq)]
pub enum AlphaMotionType {
    // linear interpolation
    Linear = 0,
    // set alpha to src value immediately
    Immediate,
}

impl TryFrom<i32> for AlphaMotionType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(AlphaMotionType::Linear),
            1 => Ok(AlphaMotionType::Immediate),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlphaMotion {
    id: u16,
    prim_id: u32,
    running: bool,
    reverse: bool,
    src_alpha: u8,
    dst_alpha: u8,
    duration: i32,
    elapsed: i32,
    anm_type: AlphaMotionType,
}

impl AlphaMotion {
    pub fn new() -> AlphaMotion {
        AlphaMotion {
            id: 0,
            prim_id: 0,
            running: false,
            reverse: false,
            src_alpha: 0,
            dst_alpha: 0,
            duration: 0,
            elapsed: 0,
            anm_type: AlphaMotionType::Linear,
        }
    }

    pub fn get_id(&self) -> u16 {
        self.id
    }

    pub fn get_prim_id(&self) -> u32 {
        self.prim_id
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn is_reverse(&self) -> bool {
        self.reverse
    }

    pub fn get_src_alpha(&self) -> u8 {
        self.src_alpha
    }

    pub fn get_dst_alpha(&self) -> u8 {
        self.dst_alpha
    }

    pub fn get_duration(&self) -> i32 {
        self.duration
    }

    pub fn get_elapsed(&self) -> i32 {
        self.elapsed
    }

    pub fn get_anm_type(&self) -> AlphaMotionType {
        self.anm_type.clone()
    }

    pub fn set_id(&mut self, id: u16) {
        self.id = id;
    }

    pub fn set_prim_id(&mut self, prim_id: u32) {
        self.prim_id = prim_id;
    }

    pub fn set_anm_type(&mut self, anm_type: AlphaMotionType) {
        self.anm_type = anm_type;
    }

    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    pub fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    pub fn set_src_alpha(&mut self, src_alpha: u8) {
        self.src_alpha = src_alpha;
    }

    pub fn set_dst_alpha(&mut self, dst_alpha: u8) {
        self.dst_alpha = dst_alpha;
    }

    pub fn set_duration(&mut self, duration: i32) {
        self.duration = duration;
    }

    pub fn set_elapsed(&mut self, elapsed: i32) {
        self.elapsed = elapsed;
    }

    pub fn update(
        &mut self,
        prim_manager: &PrimManager,
        flag: bool,      // corresponds to engine->scene.should_break
        elapsed: i32,
    ) -> bool {
        // Return value semantics from decompile: 1 = keep running, 0 = finished
        if !self.running || self.prim_id as i16 == INVAILD_PRIM_HANDLE {
            return true;
        }

        let target_id = self.prim_id as i16;

        if flag {
            let root_id = prim_manager.get_custom_root_prim_id();
            if root_id == 0 {
                return true;
            }

            let mut parent = prim_manager.get_prim(target_id).get_parent();
            if parent == INVAILD_PRIM_HANDLE {
                return true;
            }

            // Walk up until we reach root_id; if we hit invalid, gate out.
            while parent != root_id as i16 {
                parent = prim_manager.get_prim(parent).get_parent();
                if parent == INVAILD_PRIM_HANDLE {
                    return true;
                }
            }
        } else {
            if prim_manager.get_prim(target_id).get_paused() {
                return true;
            }

            let mut walker = target_id;
            loop {
                let parent = prim_manager.get_prim(walker).get_parent();
                if parent == INVAILD_PRIM_HANDLE {
                    break;
                }
                let p = prim_manager.get_prim(parent);
                if p.get_paused() {
                    return true;
                }
                walker = parent;
            }
        }

        let mut prim = prim_manager.get_prim(target_id);

        prim.apply_attr(0x40);

        let mut step = elapsed;
        if self.reverse && step < 0 {
            step = -step;
        }
        self.elapsed += step;

        if step < 0 || self.elapsed >= self.duration {
            self.running = false; 
            prim.set_alpha(self.dst_alpha);
            return false;
        }

        match self.anm_type {
            AlphaMotionType::Linear => {
                let src = self.src_alpha as i32;
                let dst = self.dst_alpha as i32;
                let alpha = src + (dst - src) * self.elapsed / self.duration;
                prim.set_alpha(alpha as u8);
            }
            _ => {
                prim.set_alpha(self.src_alpha);
            }
        }

        true
    }
}

pub struct AlphaMotionContainer {
    motions: Vec<AlphaMotion>,
    current_id: u32,
    allocation_pool: Vec<u16>,
}

impl AlphaMotionContainer {
    pub fn new() -> AlphaMotionContainer {
        let allocation_pool: Vec<u16> = (0..256).collect();

        AlphaMotionContainer {
            motions: vec![AlphaMotion::new(); 256],
            current_id: 0,
            allocation_pool,
        }
    }

    fn next_free_id(&mut self, prim_id: u32) -> Option<u32> {
        // Fast path: if this prim already has a running alpha motion, stop it and reuse its slot.
        if let Some(i) = self
            .motions
            .iter()
            .position(|m| m.is_running() && m.get_prim_id() == prim_id)
        {
            self.motions[i].set_running(false);
            if self.current_id > 0 {
                self.current_id -= 1;
            }
            self.allocation_pool[self.current_id as usize] = self.motions[i].get_id();
            return Some(self.current_id);
        }

        // Otherwise, allocate from the pool if we still have capacity.
        if (self.current_id as usize) < self.allocation_pool.len() {
            return Some(self.current_id);
        }

        None
    }

    pub fn push_motion(
        &mut self,
        prim_id: u32,
        src_alpha: u8,
        dest_alpha: u8,
        duration: i32,
        anm_type: AlphaMotionType,
        reverse: bool,
    ) -> Result<()> {
        if let Some(id) = self.next_free_id(prim_id) {
            let id = self.allocation_pool[id as usize];
            self.current_id += 1;
            let alpha_motion = &mut self.motions[id as usize];

            alpha_motion.set_id(id);
            alpha_motion.set_prim_id(prim_id);
            alpha_motion.set_running(true);
            alpha_motion.set_reverse(reverse);
            alpha_motion.set_src_alpha(src_alpha);
            alpha_motion.set_dst_alpha(dest_alpha);
            alpha_motion.set_duration(duration);
            alpha_motion.set_elapsed(0);
            alpha_motion.set_anm_type(anm_type);

            return Ok(());
        }

        anyhow::bail!("Failed to allocate new motion");
    }

    pub fn stop_motion(&mut self, prim_id: u32) -> Result<()> {
        let mut i = 0;
        while !self.motions[i].is_running() || self.motions[i].get_prim_id() != prim_id {
            i += 1;
            if i >= 256 {
                return Ok(());
            }
        }

        self.motions[i].set_running(false);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;

        Ok(())
    }

    pub fn test_motion(&self, prim_id: u32) -> bool {
        let mut i = 0;
        while !self.motions[i].is_running() || self.motions[i].get_prim_id() != prim_id {
            i += 1;
            if i >= 256 {
                return false;
            }
        }

        self.motions[i].is_running()
    }

    pub fn exec_alpha_motion(
        &mut self,
        prim_manager: &PrimManager,
        flag: bool,
        elapsed: i32,
    ) {
        for i in 0..256 {
            if !self.motions[i].is_running() {
                continue;
            }
            
            if !self.motions[i].update(prim_manager, flag, elapsed) {
                self.motions[i].set_running(false);
                if self.current_id > 0 {
                    self.current_id -= 1;
                }
                self.allocation_pool[self.current_id as usize] = self.motions[i].get_id();
            }
        }
    }
}



impl AlphaMotionContainer {
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
                "  [alpha] prim={} src={} dst={} elapsed={} dur={} type={:?} rev={}\n",
                m.prim_id,
                m.src_alpha,
                m.dst_alpha,
                m.elapsed,
                m.duration,
                m.anm_type,
                m.reverse
            ));
            n += 1;
        }
        out
    }

    pub fn debug_running_count(&self) -> usize {
        self.motions.iter().filter(|m| m.running).count()
    }
}
