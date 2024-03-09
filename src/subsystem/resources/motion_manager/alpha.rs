use std::{cell::RefCell, sync::Arc};
use anyhow::Result;
use atomic_refcell::AtomicRefCell;

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
        prim_manager: &AtomicRefCell<PrimManager>,
        flag: bool,
        elapsed: i32,
    ) -> bool {
        if !self.running || self.prim_id as i16 == INVAILD_PRIM_HANDLE {
            return true;
        }

        let mut prim_manager = prim_manager.borrow_mut();
        let mut prim = prim_manager.get_prim(self.prim_id as i16);
        let custom_root_id = prim_manager.get_custom_root_prim_id();
        if flag {
            if custom_root_id == 0 {
                return true;
            }
            let mut next = prim.get_parent();
            if next == INVAILD_PRIM_HANDLE {
                return true;
            }

            while next as u16 != custom_root_id {
                next = prim_manager.get_prim(next as i16).get_parent();
                if next == INVAILD_PRIM_HANDLE {
                    return true;
                }
            }
        } else {
            let mut prim = prim_manager.get_prim(self.prim_id as i16);
            if prim.get_paused() {
                return true;
            }

            loop {
                let next = prim.get_parent();
                if next == INVAILD_PRIM_HANDLE {
                    break;
                }
                prim = prim_manager.get_prim(next as i16);
                if !prim.get_paused() {
                    return true;
                }
            }
        }

        prim.apply_attr(0x40);
        let mut elapsed = elapsed;
        if self.reverse && elapsed < 0 {
            elapsed = -elapsed;
        }

        self.elapsed += elapsed;
        if elapsed < 0 || self.elapsed >= self.duration {
            prim.set_alpha(self.dst_alpha);
            return false;
        }

        match self.anm_type {
            AlphaMotionType::Linear => {
                let alpha = self.src_alpha as i32
                    + (self.dst_alpha as i32 - self.src_alpha as i32) * self.elapsed
                        / self.duration;
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
        let mut i = 0;
        while !self.motions[i].is_running() || self.motions[i].get_prim_id() != prim_id {
            i += 1;
            if i >= 256 {
                return None;
            }
        }

        self.motions[i].set_running(false);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;
        Some(self.current_id)
    }

    pub fn get_motions(&self) -> &Vec<AlphaMotion> {
        &self.motions
    }

    pub fn get_motions_mut(&mut self) -> &mut Vec<AlphaMotion> {
        &mut self.motions
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
        prim_manager: &AtomicRefCell<PrimManager>,
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
                self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;
            }
        }
    }
}

