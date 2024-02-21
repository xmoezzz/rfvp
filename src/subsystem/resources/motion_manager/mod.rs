mod alpha;
mod normal_move;
mod rotation_move;
mod s2_move;

use std::{cell::RefCell, sync::Arc};

use anyhow::Result;
use super::prim::{PrimManager, INVAILD_PRIM_HANDLE};
pub use super::motion_manager::alpha::{AlphaMotion, AlphaMotionType, AlphaMotionContainer};
pub use super::motion_manager::normal_move::{MoveMotion, MoveMotionType, MoveMotionContainer};
pub use super::motion_manager::rotation_move::{RotationMotion, RotationMotionType, RotationMotionContainer};
pub use super::motion_manager::s2_move::{ScaleMotion, ScaleMotionType, ScaleMotionContainer};

pub struct MotionManager {
    alpha_motion_container: AlphaMotionContainer,
    move_motion_container: MoveMotionContainer,
    rotation_motion_container: RotationMotionContainer,
    scale_motion_container: ScaleMotionContainer,
}

impl MotionManager {
    pub fn new(prim_manager: Arc<RefCell<PrimManager>>) -> MotionManager {
        MotionManager {
            alpha_motion_container: AlphaMotionContainer::new(prim_manager.clone()),
            move_motion_container: MoveMotionContainer::new(prim_manager.clone()),
            rotation_motion_container: RotationMotionContainer::new(prim_manager.clone()),
            scale_motion_container: ScaleMotionContainer::new(prim_manager.clone()),
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

    pub fn set_rotation_motion(
        &mut self,
        prim_id: u32,
        src_angle: i16,
        dest_angle: i16,
        duration: i32,
        typ: RotationMotionType,
        reverse: bool,
    ) -> Result<()> {
        self.rotation_motion_container.push_motion(
            prim_id, src_angle, dest_angle, duration, typ, reverse,
        )
    }

    pub fn stop_rotation_motion(&mut self, prim_id: u32) -> Result<()> {
        self.rotation_motion_container.stop_motion(prim_id)
    }

    pub fn test_rotation_motion(&self, prim_id: u32) -> bool {
        self.rotation_motion_container.test_motion(prim_id)
    }

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
            prim_id, src_w_factor, src_h_factor, dst_w_factor, dst_h_factor, duration, typ, reverse
        )
    }

    pub fn stop_scale_motion(&mut self, prim_id: u32) -> Result<()> {
        self.scale_motion_container.stop_motion(prim_id)
    }

    pub fn test_scale_motion(&self, prim_id: u32) -> bool {
        self.scale_motion_container.test_motion(prim_id)
    }

}
