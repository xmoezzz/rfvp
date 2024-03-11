use anyhow::Result;
use atomic_refcell::AtomicRefCell;
use std::{cell::RefCell, sync::Arc};

use crate::subsystem::resources::prim::{PrimManager, INVAILD_PRIM_HANDLE};

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
    pub fn new() -> RotationMotion {
        RotationMotion {
            id: 0,
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

    pub fn get_src_angle(&self) -> i16 {
        self.src_angle
    }

    pub fn get_dst_angle(&self) -> i16 {
        self.dst_angle
    }

    pub fn get_duration(&self) -> i32 {
        self.duration
    }

    pub fn get_elapsed(&self) -> i32 {
        self.elapsed
    }

    pub fn get_type(&self) -> RotationMotionType {
        self.typ.clone()
    }

    pub fn set_id(&mut self, id: u16) {
        self.id = id;
    }

    pub fn set_prim_id(&mut self, prim_id: u32) {
        self.prim_id = prim_id;
    }

    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    pub fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    pub fn set_src_angle(&mut self, src_angle: i16) {
        self.src_angle = src_angle;
    }

    pub fn set_dst_angle(&mut self, dst_angle: i16) {
        self.dst_angle = dst_angle;
    }

    pub fn set_duration(&mut self, duration: i32) {
        self.duration = duration;
    }

    pub fn set_elapsed(&mut self, elapsed: i32) {
        self.elapsed = elapsed;
    }

    pub fn set_type(&mut self, typ: RotationMotionType) {
        self.typ = typ;
    }

    pub fn update(
        &mut self,
        prim_manager: &AtomicRefCell<PrimManager>,
        flag: bool,
        elapsed: i32,
    ) -> bool {
        if self.get_type() == RotationMotionType::None || self.prim_id as i16 == INVAILD_PRIM_HANDLE
        {
            return true;
        }

        let prim_manager = prim_manager.borrow_mut();
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
                next = prim_manager.get_prim(next).get_parent();
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
                prim = prim_manager.get_prim(next);
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
            prim.set_rotation(self.dst_angle);
            return false;
        }

        let src_r = self.src_angle as i64;
        let dst_r = self.dst_angle as i64;
        let delta_r = dst_r - src_r;

        match self.get_type() {
            RotationMotionType::Linear => {
                let r = src_r + delta_r * self.elapsed as i64 / self.duration as i64;
                prim.set_rotation(r as i16);
            }
            RotationMotionType::Accelerate => {
                let r = src_r
                    + delta_r * self.elapsed as i64 * self.elapsed as i64
                        / (self.duration as i64 * self.duration as i64);
                prim.set_rotation(r as i16);
            }
            RotationMotionType::Decelerate => {
                let r = dst_r
                    - delta_r
                        * (self.duration as i64 - self.elapsed as i64) as i64
                        * (self.duration as i64 - self.elapsed as i64)
                        / (self.duration as i64 * self.duration as i64);
                prim.set_rotation(r as i16);
            }
            RotationMotionType::Rebound => {
                let half_delta_r = delta_r / 2;
                let half_duration = self.duration as i64 / 2;
                if elapsed as i64 > half_duration {
                    let r = dst_r
                        - (delta_r - half_delta_r)
                            * (self.duration as i64 - self.elapsed as i64)
                            * (self.duration as i64 - self.elapsed as i64)
                            / ((self.duration as i64 - half_duration)
                                * (self.duration as i64 - half_duration));
                    prim.set_rotation(r as i16);
                } else {
                    let r = src_r
                        + half_delta_r * elapsed as i64 * elapsed as i64
                            / (half_duration * half_duration);
                    prim.set_rotation(r as i16);
                }
            }
            RotationMotionType::Bounce => {
                let half_delta_r = delta_r / 2;
                let half_duration = self.duration as i64 / 2;
                if elapsed as i64 > half_duration {
                    let r = half_delta_r
                        + src_r
                        + (delta_r - half_delta_r)
                            * (self.elapsed as i64 - half_duration)
                            * (self.elapsed as i64 - half_duration)
                            / (self.duration as i64 - half_duration)
                            / (self.duration as i64 - half_duration);
                    prim.set_rotation(r as i16);
                } else {
                    let r = half_delta_r + src_r
                        - half_delta_r
                            * (half_duration - self.elapsed as i64)
                            * (half_duration - self.elapsed as i64)
                            / half_duration
                            / half_duration;
                    prim.set_rotation(r as i16);
                }
            }
            _ => {
                prim.set_rotation(self.dst_angle);
            }
        }

        true
    }
}

pub struct RotationMotionContainer {
    motions: Vec<RotationMotion>,
    current_id: u32,
    allocation_pool: Vec<u16>,
}

impl RotationMotionContainer {
    pub fn new() -> RotationMotionContainer {
        let allocation_pool: Vec<u16> = (0..512).collect();

        RotationMotionContainer {
            motions: vec![RotationMotion::new(); 512],
            current_id: 0,
            allocation_pool,
        }
    }

    fn next_free_id(&mut self, prim_id: u32) -> Option<u32> {
        let mut i = 0;
        while self.motions[i].typ == RotationMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return None;
            }
        }

        self.motions[i].set_running(false);
        self.motions[i].set_type(RotationMotionType::None);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id();
        Some(self.current_id)
    }

    pub fn get_motions(&self) -> &Vec<RotationMotion> {
        &self.motions
    }

    pub fn get_motions_mut(&mut self) -> &mut Vec<RotationMotion> {
        &mut self.motions
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
        if let Some(id) = self.next_free_id(prim_id) {
            let id = self.allocation_pool[id as usize];
            self.current_id += 1;
            let prim = &mut self.motions[id as usize];

            prim.set_id(id);
            prim.set_prim_id(prim_id);
            prim.set_running(true);
            prim.set_reverse(reverse);
            prim.set_src_angle(src_angle);
            prim.set_dst_angle(dest_angle);
            prim.set_duration(duration);
            prim.set_elapsed(0);
            prim.set_type(typ);
        }

        Ok(())
    }

    pub fn stop_motion(&mut self, prim_id: u32) -> Result<()> {
        let mut i = 0;
        while self.motions[i].get_type() == RotationMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return Ok(());
            }
        }

        self.motions[i].set_running(false);
        self.motions[i].set_type(RotationMotionType::None);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id();

        Ok(())
    }

    pub fn test_motion(&self, prim_id: u32) -> bool {
        let mut i = 0;
        while self.motions[i].get_type() != RotationMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return false;
            }
        }

        self.motions[i].get_type() != RotationMotionType::None
    }

    pub fn exec_rotation_motion(
        &mut self,
        prim_manager: &AtomicRefCell<PrimManager>,
        flag: bool,
        elapsed: i32,
    ) {
        for i in 0..512 {
            if !self.motions[i].is_running() {
                continue;
            }
            
            if !self.motions[i].update(prim_manager, flag, elapsed) {
                self.motions[i].set_running(false);
                self.motions[i].set_type(RotationMotionType::None);
                if self.current_id > 0 {
                    self.current_id -= 1;
                }
                self.allocation_pool[self.current_id as usize] = self.motions[i].get_id();
            }
        }
    }
}
