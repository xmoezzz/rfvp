use anyhow::Result;
use std::{cell::RefCell, sync::Arc};

use crate::subsystem::resources::prim::{PrimManager, INVAILD_PRIM_HANDLE};

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
    id: u32,
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
    pub fn new() -> ZMotion {
        ZMotion {
            id: 0,
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

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_prim_id(&self) -> u32 {
        self.prim_id
    }

    pub fn is_zunning(&self) -> bool {
        self.running
    }

    pub fn is_zeverse(&self) -> bool {
        self.reverse
    }

    pub fn get_src_z(&self) -> i16 {
        self.src_z
    }

    pub fn get_dst_z(&self) -> i16 {
        self.dst_z
    }

    pub fn get_duration(&self) -> i32 {
        self.duration
    }

    pub fn get_elapsed(&self) -> i32 {
        self.elapsed
    }

    pub fn get_type(&self) -> ZMotionType {
        self.typ.clone()
    }

    pub fn set_id(&mut self, id: u32) {
        self.id = id;
    }

    pub fn set_prim_id(&mut self, prim_id: u32) {
        self.prim_id = prim_id;
    }

    pub fn set_zunning(&mut self, running: bool) {
        self.running = running;
    }

    pub fn set_zeverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    pub fn set_src_z(&mut self, src_z: i16) {
        self.src_z = src_z;
    }

    pub fn set_dst_z(&mut self, dst_z: i16) {
        self.dst_z = dst_z;
    }

    pub fn set_duration(&mut self, duration: i32) {
        self.duration = duration;
    }

    pub fn set_elapsed(&mut self, elapsed: i32) {
        self.elapsed = elapsed;
    }

    pub fn set_type(&mut self, typ: ZMotionType) {
        self.typ = typ;
    }

    pub fn update(
        &mut self,
        prim_manager: &Arc<RefCell<PrimManager>>,
        flag: bool,
        elapsed: i32,
    ) -> bool {
        if self.get_type() == ZMotionType::None || self.prim_id as i16 == INVAILD_PRIM_HANDLE
        {
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
            if prim.get_flag() {
                return true;
            }

            loop {
                let next = prim.get_parent();
                if next == INVAILD_PRIM_HANDLE {
                    break;
                }
                prim = prim_manager.get_prim(next as i16);
                if !prim.get_flag() {
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
            prim.set_z(self.dst_z);
            return false;
        }

        let src_z = self.src_z as i64;
        let dst_z = self.dst_z as i64;
        let delta_z = dst_z - src_z;

        match self.get_type() {
            ZMotionType::Linear => {
                let r = src_z + delta_z * self.elapsed as i64 / self.duration as i64;
                prim.set_z(r as i16);
            }
            ZMotionType::Accelerate => {
                let r = src_z
                    + delta_z * self.elapsed as i64 * self.elapsed as i64
                        / (self.duration as i64 * self.duration as i64);
                prim.set_z(r as i16);
            }
            ZMotionType::Decelerate => {
                let r = dst_z
                    - delta_z
                        * (self.duration as i64 - self.elapsed as i64) as i64
                        * (self.duration as i64 - self.elapsed as i64)
                        / (self.duration as i64 * self.duration as i64);
                prim.set_z(r as i16);
            }
            ZMotionType::Rebound => {
                let half_delta_z = delta_z / 2;
                let half_duration = self.duration as i64 / 2;
                if elapsed as i64 > half_duration {
                    let r = dst_z
                        - (delta_z - half_delta_z)
                            * (self.duration as i64 - self.elapsed as i64)
                            * (self.duration as i64 - self.elapsed as i64)
                            / ((self.duration as i64 - half_duration)
                                * (self.duration as i64 - half_duration));
                    prim.set_z(r as i16);
                } else {
                    let r = src_z
                        + half_delta_z * elapsed as i64 * elapsed as i64
                            / (half_duration * half_duration);
                    prim.set_z(r as i16);
                }
            }
            ZMotionType::Bounce => {
                let half_delta_z = delta_z / 2;
                let half_duration = self.duration as i64 / 2;
                if elapsed as i64 > half_duration {
                    let r = half_delta_z
                        + src_z
                        + (delta_z - half_delta_z)
                            * (self.elapsed as i64 - half_duration)
                            * (self.elapsed as i64 - half_duration)
                            / (self.duration as i64 - half_duration)
                            / (self.duration as i64 - half_duration);
                    prim.set_z(r as i16);
                } else {
                    let r = half_delta_z + src_z
                        - half_delta_z
                            * (half_duration - self.elapsed as i64)
                            * (half_duration - self.elapsed as i64)
                            / half_duration
                            / half_duration;
                    prim.set_z(r as i16);
                }
            }
            _ => {
                prim.set_z(self.dst_z);
            }
        }

        true
    }
}

pub struct ZMotionContainer {
    motions: Vec<ZMotion>,
    current_id: u32,
    allocation_pool: Vec<u16>,
    prim_manager: Arc<RefCell<PrimManager>>,
}

impl ZMotionContainer {
    pub fn new(prim_manager: Arc<RefCell<PrimManager>>) -> ZMotionContainer {
        let allocation_pool: Vec<u16> = (0..512).collect();

        ZMotionContainer {
            motions: vec![ZMotion::new(); 512],
            current_id: 0,
            allocation_pool,
            prim_manager,
        }
    }

    fn next_free_id(&mut self, prim_id: u32) -> Option<u32> {
        let mut i = 0;
        while self.motions[i].typ == ZMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return None;
            }
        }

        self.motions[i].set_zunning(false);
        self.motions[i].set_type(ZMotionType::None);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;
        Some(self.current_id)
    }

    pub fn get_motions(&self) -> &Vec<ZMotion> {
        &self.motions
    }

    pub fn get_motions_mut(&mut self) -> &mut Vec<ZMotion> {
        &mut self.motions
    }

    pub fn push_motion(
        &mut self,
        prim_id: u32,
        src_z: i16,
        dest_z: i16,
        duration: i32,
        typ: ZMotionType,
        reverse: bool,
    ) -> Result<()> {
        if let Some(id) = self.next_free_id(prim_id) {
            let mut prim = &mut self.motions[id as usize];

            prim.set_id(id);
            prim.set_prim_id(prim_id);
            prim.set_zunning(true);
            prim.set_zeverse(reverse);
            prim.set_src_z(src_z);
            prim.set_dst_z(dest_z);
            prim.set_duration(duration);
            prim.set_elapsed(0);
            prim.set_type(typ);
        }

        Ok(())
    }

    pub fn stop_motion(&mut self, prim_id: u32) -> Result<()> {
        let mut i = 0;
        while self.motions[i].get_type() == ZMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return Ok(());
            }
        }

        self.motions[i].set_zunning(false);
        self.motions[i].set_type(ZMotionType::None);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;

        Ok(())
    }

    pub fn test_motion(&self, prim_id: u32) -> bool {
        let mut i = 0;
        while self.motions[i].get_type() != ZMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return false;
            }
        }

        self.motions[i].get_type() != ZMotionType::None
    }
}
