mod alpha;
mod normal_move;
mod rotation_move;
mod s2_move;
mod z_move;

use std::{cell::RefCell, sync::Arc};

pub use super::motion_manager::alpha::{AlphaMotion, AlphaMotionContainer, AlphaMotionType};
pub use super::motion_manager::normal_move::{MoveMotion, MoveMotionContainer, MoveMotionType};
pub use super::motion_manager::rotation_move::{
    RotationMotion, RotationMotionContainer, RotationMotionType,
};
pub use super::motion_manager::s2_move::{ScaleMotion, ScaleMotionContainer, ScaleMotionType};
pub use super::motion_manager::z_move::{ZMotion, ZMotionContainer, ZMotionType};
use super::prim::{PrimManager, INVAILD_PRIM_HANDLE};
use anyhow::Result;

pub struct MotionManager {
    alpha_motion_container: AlphaMotionContainer,
    move_motion_container: MoveMotionContainer,
    rotation_motion_container: RotationMotionContainer,
    scale_motion_container: ScaleMotionContainer,
    z_motion_container: ZMotionContainer,
}

impl MotionManager {
    pub fn new(prim_manager: Arc<RefCell<PrimManager>>) -> MotionManager {
        MotionManager {
            alpha_motion_container: AlphaMotionContainer::new(prim_manager.clone()),
            move_motion_container: MoveMotionContainer::new(prim_manager.clone()),
            rotation_motion_container: RotationMotionContainer::new(prim_manager.clone()),
            scale_motion_container: ScaleMotionContainer::new(prim_manager.clone()),
            z_motion_container: ZMotionContainer::new(prim_manager.clone()),
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

    #[allow(clippy::too_many_arguments)]
    pub fn set_move_motion(
        &mut self,
        prim_id: u32,
        src_x: u32,
        dst_x: u32,
        src_y: u32,
        dst_y: u32,
        duration: i32,
        anm_type: MoveMotionType,
        reverse: bool,
    ) -> Result<()> {
        self.move_motion_container.push_motion(
            prim_id, src_x, src_y, dst_x, dst_y, duration, anm_type, reverse,
        )
    }

    pub fn stop_move_motion(&mut self, prim_id: u32) -> Result<()> {
        self.move_motion_container.stop_motion(prim_id)
    }

    pub fn test_move_motion(&self, prim_id: u32) -> bool {
        self.move_motion_container.test_motion(prim_id)
    }

    pub fn set_rotation_motion(
        &mut self,
        prim_id: u32,
        src_angle: i16,
        dest_angle: i16,
        duration: i32,
        typ: RotationMotionType,
        reverse: bool,
    ) -> Result<()> {
        self.rotation_motion_container
            .push_motion(prim_id, src_angle, dest_angle, duration, typ, reverse)
    }

    pub fn stop_rotation_motion(&mut self, prim_id: u32) -> Result<()> {
        self.rotation_motion_container.stop_motion(prim_id)
    }

    pub fn test_rotation_motion(&self, prim_id: u32) -> bool {
        self.rotation_motion_container.test_motion(prim_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_scale_motion(
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
        self.scale_motion_container.push_motion(
            prim_id,
            src_w_factor,
            src_h_factor,
            dst_w_factor,
            dst_h_factor,
            duration,
            typ,
            reverse,
        )
    }

    pub fn stop_scale_motion(&mut self, prim_id: u32) -> Result<()> {
        self.scale_motion_container.stop_motion(prim_id)
    }

    pub fn test_scale_motion(&self, prim_id: u32) -> bool {
        self.scale_motion_container.test_motion(prim_id)
    }

    pub fn set_z_motion(
        &mut self,
        prim_id: u32,
        src_z: i32,
        dst_z: i32,
        duration: i32,
        typ: ZMotionType,
        reverse: bool,
    ) -> Result<()> {
        self.z_motion_container
            .push_motion(prim_id, src_z as i16, dst_z as i16, duration, typ, reverse)
    }

    pub fn stop_z_motion(&mut self, prim_id: u32) -> Result<()> {
        self.z_motion_container.stop_motion(prim_id)
    }

    pub fn test_z_motion(&self, prim_id: u32) -> bool {
        self.z_motion_container.test_motion(prim_id)
    }
}
