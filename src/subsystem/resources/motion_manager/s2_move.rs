use anyhow::Result;
use atomic_refcell::AtomicRefCell;

use crate::subsystem::resources::prim::{PrimManager, INVAILD_PRIM_HANDLE};

#[derive(Debug, Clone, PartialEq)]
pub enum ScaleMotionType {
    None = 0,
    Linear,
    Accelerate,
    Decelerate,
    Rebound,
    Bounce,
}

impl TryFrom<i32> for ScaleMotionType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ScaleMotionType::None),
            1 => Ok(ScaleMotionType::Linear),
            2 => Ok(ScaleMotionType::Accelerate),
            3 => Ok(ScaleMotionType::Decelerate),
            4 => Ok(ScaleMotionType::Rebound),
            5 => Ok(ScaleMotionType::Bounce),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScaleMotion {
    id: u16,
    prim_id: u32,
    running: bool,
    reverse: bool,
    src_w_factor: i32,
    src_h_factor: i32,
    dst_w_factor: i32,
    dst_h_factor: i32,
    duration: i32,
    elapsed: i32,
    typ: ScaleMotionType,
}

impl ScaleMotion {
    pub fn new() -> ScaleMotion {
        ScaleMotion {
            id: 0,
            prim_id: 0,
            running: false,
            reverse: false,
            src_w_factor: 0,
            src_h_factor: 0,
            dst_w_factor: 0,
            dst_h_factor: 0,
            duration: 0,
            elapsed: 0,
            typ: ScaleMotionType::None,
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

    pub fn get_src_w_factor(&self) -> i32 {
        self.src_w_factor
    }

    pub fn get_src_h_factor(&self) -> i32 {
        self.src_h_factor
    }

    pub fn get_dst_w_factor(&self) -> i32 {
        self.dst_w_factor
    }

    pub fn get_dst_h_factor(&self) -> i32 {
        self.dst_h_factor
    }

    pub fn get_duration(&self) -> i32 {
        self.duration
    }

    pub fn get_elapsed(&self) -> i32 {
        self.elapsed
    }

    pub fn get_type(&self) -> ScaleMotionType {
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

    pub fn set_src_w_factor(&mut self, src_w_factor: i32) {
        self.src_w_factor = src_w_factor;
    }

    pub fn set_src_h_factor(&mut self, src_h_factor: i32) {
        self.src_h_factor = src_h_factor;
    }

    pub fn set_dst_w_factor(&mut self, dst_w_factor: i32) {
        self.dst_w_factor = dst_w_factor;
    }

    pub fn set_dst_h_factor(&mut self, dst_h_factor: i32) {
        self.dst_h_factor = dst_h_factor;
    }

    pub fn set_duration(&mut self, duration: i32) {
        self.duration = duration;
    }

    pub fn set_elapsed(&mut self, elapsed: i32) {
        self.elapsed = elapsed;
    }

    pub fn set_type(&mut self, typ: ScaleMotionType) {
        self.typ = typ;
    }

    pub fn update(
        &mut self,
        prim_manager: &PrimManager,
        flag: bool,
        elapsed: i32,
    ) -> bool {
        if self.get_type() == ScaleMotionType::None || self.prim_id as i16 == INVAILD_PRIM_HANDLE
        {
            return true;
        }

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
            prim.set_factor_x(self.dst_w_factor as i16);
            prim.set_factor_y(self.dst_h_factor as i16);
            return false;
        }

        let src_w_factor = self.src_w_factor as i64;
        let src_h_factor = self.src_h_factor as i64;
        let dst_w_factor = self.dst_w_factor as i64;
        let dst_h_factor = self.dst_h_factor as i64;
        let delta_w_factor = dst_w_factor - src_w_factor;
        let delta_h_factor = dst_h_factor - src_h_factor;

        match self.get_type() {
            ScaleMotionType::Linear => {
                let factor_x = src_w_factor
                    + delta_w_factor * self.elapsed as i64 / self.duration as i64;
                let factor_y = src_h_factor
                    + delta_h_factor * self.elapsed as i64 / self.duration as i64;

                prim.set_factor_x(factor_x as i16);
                prim.set_factor_y(factor_y as i16);
            }
            ScaleMotionType::Accelerate => {
                let factor_x = src_w_factor
                    + delta_w_factor * self.elapsed as i64 * self.elapsed as i64
                        / (self.duration as i64 * self.duration as i64);
                let factor_y = src_h_factor
                    + delta_h_factor * self.elapsed as i64 * self.elapsed as i64
                        / (self.duration as i64 * self.duration as i64);

                prim.set_factor_x(factor_x as i16);
                prim.set_factor_y(factor_y as i16);
            }
            ScaleMotionType::Decelerate => {
                let numerator = (self.duration as i64 - self.elapsed as i64) * (self.duration as i64 - self.elapsed as i64);
                let factor_x = src_w_factor - delta_w_factor * numerator / (self.duration as i64 * self.duration as i64);
                let factor_y = src_h_factor - delta_h_factor * numerator / (self.duration as i64 * self.duration as i64);

                prim.set_factor_x(factor_x as i16);
                prim.set_factor_y(factor_y as i16);
            }
            ScaleMotionType::Rebound => {
                let half_delta_w_factor = delta_w_factor / 2;
                let half_delta_h_factor = delta_h_factor / 2;
                let half_duration = self.duration as i64 / 2;
                if self.elapsed as i64 > half_duration {
                    let remain = self.duration as i64 - self.elapsed as i64;
                    let time2 = self.duration as i64 - half_duration;
                    let factor_x = dst_w_factor - (delta_w_factor - half_delta_w_factor) * remain * remain / (time2 * time2);
                    let factor_y = dst_h_factor - (delta_h_factor - half_delta_h_factor) * remain * remain / (time2 * time2);
                    prim.set_factor_x(factor_x as i16);
                    prim.set_factor_y(factor_y as i16);
                }
                else
                {
                    let time2 = self.elapsed as i64 * self.elapsed as i64;
                    let factor_x = src_w_factor + half_delta_w_factor * time2 / (half_duration * half_duration);
                    let factor_y = src_h_factor + half_delta_h_factor * time2 / (half_duration * half_duration);
                    prim.set_factor_x(factor_x as i16);
                    prim.set_factor_y(factor_y as i16);
                }
            }
            ScaleMotionType::Bounce => {
                let half_delta_w_factor = delta_w_factor / 2;
                let half_delta_h_factor = delta_h_factor / 2;
                let half_duration = self.duration as i64 / 2;
                
                if self.elapsed as i64 > self.duration as i64 / 2 {
                    let remian = self.elapsed as i64 - half_duration;
                    let time2 = self.duration as i64 - half_duration;
                    let factor_x = half_delta_w_factor
                                    + src_w_factor
                                    + (delta_w_factor - half_delta_w_factor) * remian * remian / (time2 * time2);
                    let factor_y = half_delta_h_factor
                                    + src_h_factor
                                    + (delta_h_factor - half_delta_h_factor) * remian * remian / (time2 * time2);

                    prim.set_factor_x(factor_x as i16);
                    prim.set_factor_y(factor_y as i16);
                }
                else {
                    let rev_remian = half_duration - self.elapsed as i64;
                    let factor_x = half_delta_w_factor
                                + src_w_factor
                                 - half_delta_w_factor * rev_remian * rev_remian / (half_duration * half_duration);
                    let factor_y = half_delta_h_factor
                                + src_h_factor
                                 - half_delta_h_factor * rev_remian * rev_remian / (half_duration * half_duration);

                    prim.set_factor_x(factor_x as i16);
                    prim.set_factor_y(factor_y as i16);
                }
            }
            _ => {
                prim.set_factor_x(dst_w_factor as i16);
                prim.set_factor_y(dst_h_factor as i16);
            }
        }

        true
    }
}

pub struct ScaleMotionContainer {
    motions: Vec<ScaleMotion>,
    current_id: u32,
    allocation_pool: Vec<u16>,
}

impl ScaleMotionContainer {
    pub fn new() -> ScaleMotionContainer {
        let allocation_pool: Vec<u16> = (0..512).collect();

        ScaleMotionContainer {
            motions: vec![ScaleMotion::new(); 512],
            current_id: 0,
            allocation_pool,
        }
    }

    fn next_free_id(&mut self, prim_id: u32) -> Option<u32> {
        let mut i = 0;
        while self.motions[i].typ == ScaleMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return None;
            }
        }

        self.motions[i].set_running(false);
        self.motions[i].set_type(ScaleMotionType::None);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id();
        Some(self.current_id)
    }

    pub fn get_motions(&self) -> &Vec<ScaleMotion> {
        &self.motions
    }

    pub fn get_motions_mut(&mut self) -> &mut Vec<ScaleMotion> {
        &mut self.motions
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_motion(
        &mut self,
        prim_id: u32,
        src_w_factor: i32,
        src_h_factor: i32,
        dst_w_factor: i32,
        dst_h_factor: i32,
        duration: i32,
        typ: ScaleMotionType,
        reverse: bool,
    ) -> Result<()> {
        if let Some(id) = self.next_free_id(prim_id) {
            let mut id = self.allocation_pool[id as usize];
            self.current_id += 1;
            let mut prim = &mut self.motions[id as usize];

            prim.set_id(id);
            prim.set_prim_id(prim_id);
            prim.set_running(true);
            prim.set_reverse(false);
            prim.set_src_w_factor(src_w_factor);
            prim.set_src_h_factor(src_h_factor);
            prim.set_dst_w_factor(dst_w_factor);
            prim.set_dst_h_factor(dst_h_factor);
            prim.set_duration(duration);
            prim.set_elapsed(0);
            prim.set_type(typ);
            prim.set_reverse(reverse);
        }
        Ok(())
    }

    pub fn stop_motion(&mut self, prim_id: u32) -> Result<()> {
        let mut i = 0;
        while self.motions[i].get_type() == ScaleMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return Ok(());
            }
        }

        self.motions[i].set_running(false);
        self.motions[i].set_type(ScaleMotionType::None);
        if self.current_id > 0 {
            self.current_id -= 1;
        }
        self.allocation_pool[self.current_id as usize] = self.motions[i].get_id();

        Ok(())
    }


    pub fn test_motion(&self, prim_id: u32) -> bool {
        let mut i = 0;
        while self.motions[i].get_type() != ScaleMotionType::None
            || self.motions[i].get_prim_id() != prim_id
        {
            i += 1;
            if i >= 512 {
                return false;
            }
        }

        self.motions[i].get_type() != ScaleMotionType::None
    }

    pub fn exec_s2_motion(
        &mut self,
        prim_manager: &PrimManager,
        flag: bool,
        elapsed: i32,
    ) {
        for i in 0..512 {
            if !self.motions[i].is_running() {
                continue;
            }
            
            if !self.motions[i].update(prim_manager, flag, elapsed) {
                self.motions[i].set_running(false);
                self.motions[i].set_type(ScaleMotionType::None);
                if self.current_id > 0 {
                    self.current_id -= 1;
                }
                self.allocation_pool[self.current_id as usize] = self.motions[i].get_id() as u16;
            }
        }
    }
}
