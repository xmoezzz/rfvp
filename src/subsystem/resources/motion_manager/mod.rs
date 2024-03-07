mod alpha;
mod normal_move;
mod rotation_move;
mod s2_move;
mod z_move;
mod v3d;

use std::{cell::RefCell, sync::Arc};

pub use super::motion_manager::alpha::{AlphaMotionContainer, AlphaMotionType};
pub use super::motion_manager::normal_move::{MoveMotionContainer, MoveMotionType};
pub use super::motion_manager::rotation_move::{
    RotationMotionContainer, RotationMotionType,
};
pub use super::motion_manager::s2_move::{ScaleMotionContainer, ScaleMotionType};
pub use super::motion_manager::z_move::{ZMotionContainer, ZMotionType};
pub use super::motion_manager::v3d::{V3dMotionContainer, V3dMotionType};
use super::parts_manager::PartsManager;
use super::prim::{PrimManager, INVAILD_PRIM_HANDLE};
use anyhow::Result;
use atomic_refcell::AtomicRefCell;

pub struct MotionManager {
    alpha_motion_container: AlphaMotionContainer,
    move_motion_container: MoveMotionContainer,
    rotation_motion_container: RotationMotionContainer,
    scale_motion_container: ScaleMotionContainer,
    z_motion_container: ZMotionContainer,
    v3d_motion_container: V3dMotionContainer,
    pub(crate) prim_manager: AtomicRefCell<PrimManager>,
    pub(crate) parts_manager: AtomicRefCell<PartsManager>,
}

impl Default for MotionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MotionManager {
    pub fn new() -> MotionManager {
        let prim_manager = AtomicRefCell::new(PrimManager::new());
        let parts_manager = AtomicRefCell::new(PartsManager::new());

        MotionManager {
            alpha_motion_container: AlphaMotionContainer::new(),
            move_motion_container: MoveMotionContainer::new(),
            rotation_motion_container: RotationMotionContainer::new(),
            scale_motion_container: ScaleMotionContainer::new(),
            z_motion_container: ZMotionContainer::new(),
            v3d_motion_container: V3dMotionContainer::new(),
            prim_manager,
            parts_manager,
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

    pub fn set_v3d_motion(
        &mut self,
        dest_x: i32,
        dest_y: i32,
        dest_z: i32,
        duration: i32,
        typ: V3dMotionType,
        reverse: bool,
    ) -> Result<()> {
        self.v3d_motion_container
            .set_motion(dest_x, dest_y, dest_z, duration, typ, reverse)
    }

    pub fn stop_v3d_motion(&mut self) -> Result<()> {
        self.v3d_motion_container.stop_motion()
    }

    pub fn test_v3d_motion(&self) -> bool {
        self.v3d_motion_container.test_motion()
    }

    pub fn set_v3d(&mut self, x: i32, y: i32, z: i32) {
        self.v3d_motion_container.set_v3d(x, y, z)
    }

    pub fn set_v3d_motion_paused(&mut self, pause: bool) {
        self.v3d_motion_container.set_paused(pause)
    }

    pub fn get_v3d_motion_paused(&self) -> bool {
        self.v3d_motion_container.get_paused()
    }

    pub fn get_v3d_x(&self) -> i32 {
        self.v3d_motion_container.get_x()
    }

    pub fn get_v3d_y(&self) -> i32 {
        self.v3d_motion_container.get_y()
    }

    pub fn get_v3d_z(&self) -> i32 {
        self.v3d_motion_container.get_z()
    }
}
