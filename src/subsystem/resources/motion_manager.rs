use std::{cell::RefCell, sync::Arc};

use anyhow::Result;

use super::prim::{self, PrimManager, INVAILD_PRIM_HANDLE};

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
        prim_manager: &Arc<RefCell<PrimManager>>,
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
    id: u32,
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

    pub fn set_id(&mut self, id: u32) {
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
        prim_manager: &Arc<RefCell<PrimManager>>,
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
                    let x = half_delta_x as i64 + src_x as i64
                        - half_delta_x * time2 * time2 / half_duration * half_duration;
                    let y = half_delta_y as i64 + src_y as i64
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
    prim_manager: Arc<RefCell<PrimManager>>,
}

impl MoveMotionContainer {
    pub fn new(prim_manager: Arc<RefCell<PrimManager>>) -> MoveMotionContainer {
        let allocation_pool: Vec<u16> = (0..4096).collect();

        MoveMotionContainer {
            motions: vec![MoveMotion::new(); 4096],
            current_id: 0,
            allocation_pool,
            prim_manager,
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

pub struct RotationMotion {
    id: u32,
    prim_id: u32,
    running: bool,
    reverse: bool,
    src_angle: i16,
    dst_angle: i16,
    duration: i32,
    elapsed: i32,
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

    pub fn set_id(&mut self, id: u32) {
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
}

pub struct RotationMotionContainer {
    motions: Vec<RotationMotion>,
    current_id: u32,
    allocation_pool: Vec<u16>,
    prim_manager: Arc<RefCell<PrimManager>>,
}

pub struct MotionManager {
    alpha_motion_container: AlphaMotionContainer,
    move_motion_container: MoveMotionContainer,
}

impl MotionManager {
    pub fn new(prim_manager: Arc<RefCell<PrimManager>>) -> MotionManager {
        MotionManager {
            alpha_motion_container: AlphaMotionContainer::new(prim_manager.clone()),
            move_motion_container: MoveMotionContainer::new(prim_manager.clone()),
        }
    }

    pub fn set_alpha_motion(
        &mut self,
        prim_id: u32,
        src_alpha: u8,
        dest_alpha: u8,
        duration: i32,
        anm_type: AlphaMotionType,
        reverse: bool,
    ) -> Result<()> {
        self.alpha_motion_container
            .push_motion(prim_id, src_alpha, dest_alpha, duration, anm_type, reverse)
    }

    pub fn stop_alpha_motion(&mut self, prim_id: u32) -> Result<()> {
        self.alpha_motion_container.stop_motion(prim_id)
    }

    pub fn test_alpha_motion(&self, prim_id: u32) -> bool {
        self.alpha_motion_container.test_motion(prim_id)
    }

    pub fn set_move_motion(
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
        self.move_motion_container.push_motion(
            prim_id, src_x, src_y, dst_x, dst_y, duration, anm_type, reverse,
        )
    }
}
