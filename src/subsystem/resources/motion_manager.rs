use std::{cell::RefCell, sync::Arc};

use anyhow::Result;

use super::prim::{PrimManager, INVAILD_PRIM_HANDLE};

#[derive(Debug, Clone, PartialEq)]
pub enum AlphaMotionType {
    // set alpha to destination value immediately
    None,
    // linear interpolation
    Linear,
}

#[derive(Debug, Clone)]
pub struct AlphaMotion {
    id: u32,
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

    pub fn get_id(&self) -> u32 {
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

    pub fn set_anm_type(&mut self, anm_type: AlphaMotionType) {
        self.anm_type = anm_type;
    }

    fn update_elapsed(&mut self, elapsed: i32) {
        self.elapsed += elapsed;
        if self.elapsed > self.duration {
            self.elapsed = self.duration;
            self.running = false;
            self.anm_type = AlphaMotionType::None;
        }
    }

    pub fn update(&mut self, prim_manager: &Arc<RefCell<PrimManager>>, elapsed: i32) {
        if !self.running || self.prim_id as i16 == INVAILD_PRIM_HANDLE {
            return;
        }

        let mut prim_manager = prim_manager.borrow_mut();
        let mut prim = prim_manager.get_prim(self.prim_id as i16);

    }

}

pub struct AlphaMotionContainer {
    motions: Vec<AlphaMotion>,
    current_id: u32,
    allocation_pool: Vec<u16>,
    prim_manager: Arc<RefCell<PrimManager>>,
}

impl AlphaMotionContainer {
    pub fn new(prim_manager: Arc<RefCell<PrimManager>>) -> AlphaMotionContainer {
        let allocation_pool: Vec<u16> = (0..256).collect();

        AlphaMotionContainer {
            motions: vec![AlphaMotion::new(); 256],
            current_id: 0,
            allocation_pool,
            prim_manager,
        }
    }

    fn next_free_id(&mut self, prim_id: u32) -> Option<u32> {
        let mut i = 0;
        while self.motions[i].get_anm_type() != AlphaMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 256 {
                return None;
            }
        }

        self.motions[i].set_anm_type(AlphaMotionType::None);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;
        None
    }

    pub fn get_motions(&self) -> &Vec<AlphaMotion> {
        &self.motions
    }

    pub fn get_motions_mut(&mut self) -> &mut Vec<AlphaMotion> {
        &mut self.motions
    }

    pub fn push_motion(&mut self, prim_id: u32, src_alpha: u8, dest_alpha: u8, duration: i32, anm_type: AlphaMotionType, reverse: bool) -> Result<()> {
        if let Some(id) = self.next_free_id(prim_id) {
            self.motions[id as usize].id = id;
            self.motions[id as usize].prim_id = prim_id;
            self.motions[id as usize].running = true;
            self.motions[id as usize].reverse = false;
            self.motions[id as usize].src_alpha = src_alpha;
            self.motions[id as usize].dst_alpha = dest_alpha;
            self.motions[id as usize].duration = duration;
            self.motions[id as usize].elapsed = 0;
            self.motions[id as usize].anm_type = anm_type;
            return Ok(());
        }

        anyhow::bail!("Failed to allocate new motion");
    }
}

pub enum MoveMotionType {
    None = 0,
    Linear,
    Accelerate,
    Decelerate,
    Rebound,
    Bounce,
}
