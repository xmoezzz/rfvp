use std::{cell::RefCell, sync::Arc};
use anyhow::Result;
use atomic_refcell::AtomicRefCell;

use crate::subsystem::resources::prim::{PrimManager, INVAILD_PRIM_HANDLE};



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
    src_x: u32,
    src_y: u32,
    dst_x: u32,
    dst_y: u32,
    duration: i32,
    elapsed: i32,
    anm_type: MoveMotionType,
}

impl MoveMotion {
    pub fn new() -> MoveMotion {
        MoveMotion {
            id: 0,
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

    pub fn get_src_x(&self) -> u32 {
        self.src_x
    }

    pub fn get_src_y(&self) -> u32 {
        self.src_y
    }

    pub fn get_dst_x(&self) -> u32 {
        self.dst_x
    }

    pub fn get_dst_y(&self) -> u32 {
        self.dst_y
    }

    pub fn get_duration(&self) -> i32 {
        self.duration
    }

    pub fn get_elapsed(&self) -> i32 {
        self.elapsed
    }

    pub fn get_anm_type(&self) -> MoveMotionType {
        self.anm_type.clone()
    }

    pub fn set_id(&mut self, id: u16) {
        self.id = id;
    }

    pub fn set_prim_id(&mut self, prim_id: u32) {
        self.prim_id = prim_id;
    }

    pub fn set_anm_type(&mut self, anm_type: MoveMotionType) {
        self.anm_type = anm_type;
    }

    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    pub fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    pub fn set_src_x(&mut self, src_x: u32) {
        self.src_x = src_x;
    }

    pub fn set_src_y(&mut self, src_y: u32) {
        self.src_y = src_y;
    }

    pub fn set_dst_x(&mut self, dst_x: u32) {
        self.dst_x = dst_x;
    }

    pub fn set_dst_y(&mut self, dst_y: u32) {
        self.dst_y = dst_y;
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
        if self.anm_type == MoveMotionType::None || self.prim_id as i16 == INVAILD_PRIM_HANDLE {
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
            prim.set_x(self.dst_x as i16);
            prim.set_y(self.dst_y as i16);
            return false;
        }

        let src_x = self.src_x as i32;
        let src_y = self.src_y as i32;
        let dst_x = self.dst_x as i32;
        let dst_y = self.dst_y as i32;

        let delta_x = dst_x - src_x;
        let delta_y = dst_y - src_y;

        match self.anm_type {
            MoveMotionType::Linear => {
                let x = src_x as i64 + delta_x as i64 * self.elapsed as i64 / self.duration as i64;
                let y = src_y as i64 + delta_y as i64 * self.elapsed as i64 / self.duration as i64;
                prim.set_x(x as i16);
                prim.set_y(y as i16);
            }
            MoveMotionType::Accelerate => {
                let x = src_x as i64
                    + delta_x as i64 * self.elapsed as i64 * self.elapsed as i64
                        / (self.duration as i64 * self.duration as i64);
                let y = src_y as i64
                    + delta_y as i64 * self.elapsed as i64 * self.elapsed as i64
                        / (self.duration as i64 * self.duration as i64);
                prim.set_x(x as i16);
                prim.set_y(y as i16);
            }
            MoveMotionType::Decelerate => {
                let x = dst_x as i64
                    - delta_x as i64
                        * (self.duration as i64 - self.elapsed as i64)
                        * (self.duration as i64 - self.elapsed as i64)
                        / (self.duration as i64 * self.duration as i64);
                let y = dst_y as i64
                    - delta_y as i64
                        * (self.duration as i64 - self.elapsed as i64)
                        * (self.duration as i64 - self.elapsed as i64)
                        / (self.duration as i64 * self.duration as i64);
                prim.set_x(x as i16);
                prim.set_y(y as i16);
            }
            MoveMotionType::Rebound => {
                let half_delta_x = delta_x as i64 / 2;
                let half_delta_y = delta_y as i64 / 2;
                if elapsed > self.duration / 2 {
                    let remian = self.duration as i64 - self.elapsed as i64;
                    let time2 = self.duration as i64 - self.duration as i64 / 2;
                    let x = dst_x as i64
                        - (delta_x as i64 - half_delta_x) * remian * remian / (time2 * time2);
                    let y = dst_y as i64
                        - (delta_y as i64 - half_delta_y) * remian * remian / (time2 * time2);
                    prim.set_x(x as i16);
                    prim.set_y(y as i16);
                } else {
                    let square_elapsed = self.elapsed as i64 * self.elapsed as i64;
                    let x = src_x as i64
                        + half_delta_x * square_elapsed / (self.duration as i64 / 2)
                            * (self.duration as i64 / 2);
                    let y = src_y as i64
                        + half_delta_y * square_elapsed / (self.duration as i64 / 2)
                            * (self.duration as i64 / 2);
                    prim.set_x(x as i16);
                    prim.set_y(y as i16);
                }
            }
            MoveMotionType::Bounce => {
                let half_delta_x = delta_x as i64 / 2;
                let half_delta_y = delta_y as i64 / 2;
                let half_duration = self.duration as i64 / 2;
                if elapsed as i64 > half_duration {
                    let remain = self.duration as i64 - self.elapsed as i64;
                    let time2 = self.duration as i64 - half_duration;
                    let x = half_delta_x + src_x as i64
                        - (delta_x as i64 - half_delta_x) * remain * remain / (time2 * time2);
                    let y = half_delta_y + src_y as i64
                        - (delta_y as i64 - half_delta_y) * remain * remain / (time2 * time2);
                    prim.set_x(x as i16);
                    prim.set_y(y as i16);
                } else {
                    let time2 = half_duration - self.elapsed as i64;
                    let x = half_delta_x + src_x as i64
                        - half_delta_x * time2 * time2 / half_duration * half_duration;
                    let y = half_delta_y + src_y as i64
                        - half_delta_y * time2 * time2 / half_duration * half_duration;
                    prim.set_x(x as i16);
                    prim.set_y(y as i16);
                }
            }
            _ => {
                prim.set_x(src_x as i16);
                prim.set_y(src_y as i16);
            }
        }

        true
    }
}

pub struct MoveMotionContainer {
    motions: Vec<MoveMotion>,
    current_id: u32,
    allocation_pool: Vec<u16>,
}

impl MoveMotionContainer {
    pub fn new() -> MoveMotionContainer {
        let allocation_pool: Vec<u16> = (0..4096).collect();

        MoveMotionContainer {
            motions: vec![MoveMotion::new(); 4096],
            current_id: 0,
            allocation_pool,
        }
    }

    fn next_free_id(&mut self, prim_id: u32) -> Option<u32> {
        let mut i = 0;
        while self.motions[i].get_anm_type() != MoveMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 4096 {
                return None;
            }
        }

        self.motions[i].set_running(false);
        self.motions[i].set_anm_type(MoveMotionType::None);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        let id = self.motions[i].get_id() as u16;
        self.allocation_pool[self.current_id as usize] = id;
        Some(id as u32)
    }

    pub fn get_motions(&self) -> &Vec<MoveMotion> {
        &self.motions
    }

    pub fn get_motions_mut(&mut self) -> &mut Vec<MoveMotion> {
        &mut self.motions
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_motion(
        &mut self,
        prim_id: u32,
        src_x: u32,
        src_y: u32,
        dst_x: u32,
        dst_y: u32,
        duration: i32,
        anm_type: MoveMotionType,
        reverse: bool,
    ) -> Result<()> {
        if let Some(id) = self.next_free_id(prim_id) {
            let id = self.allocation_pool[id as usize];
            self.current_id += 1;
            let mut prim = &mut self.motions[id as usize];

            prim.set_id(id);
            prim.set_prim_id(prim_id);
            prim.set_running(true);
            prim.set_reverse(reverse);
            prim.set_src_x(src_x);
            prim.set_src_y(src_y);
            prim.set_dst_x(dst_x);
            prim.set_dst_y(dst_y);
            prim.set_duration(duration);
            prim.set_elapsed(0);
            prim.set_anm_type(anm_type);
        }

        Ok(())
    }

    pub fn stop_motion(&mut self, prim_id: u32) -> Result<()> {
        let mut i = 0;
        while self.motions[i].get_anm_type() == MoveMotionType::None || self.motions[i].get_prim_id() != prim_id {
            i += 1;
            if i >= 4096 {
                return Ok(());
            }
        }

        if self.current_id > 0 {
            self.current_id -= 1;
        }

        self.motions[i].set_running(false);
        self.motions[i].set_anm_type(MoveMotionType::None);
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;

        Ok(())
    }

    pub fn test_motion(&self, prim_id: u32) -> bool {
        for i in 0..4096 {
            if self.motions[i].get_prim_id() == prim_id && self.motions[i].get_anm_type() != MoveMotionType::None {
                return true;
            }
        }

        false
    }

    pub fn exec_move_motion(
        &mut self,
        prim_manager: &AtomicRefCell<PrimManager>,
        flag: bool,
        elapsed: i32,
    ) {
        for i in 0..4096 {
            if !self.motions[i].is_running() {
                continue;
            }
            
            if !self.motions[i].update(prim_manager, flag, elapsed) {
                self.motions[i].set_running(false);
                self.motions[i].set_anm_type(MoveMotionType::None);
                if self.current_id > 0 {
                    self.current_id -= 1;
                }
                self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;
            }
        }
    }
}

