mod alpha;
mod normal_move;
mod rotation_move;
mod s2_move;
mod v3d;
mod z_move;
mod snow;

use self::snow::SnowMotionContainer;

use super::gaiji_manager::GaijiManager;
use super::graph_buff::{copy_rect, GraphBuff};
pub use super::motion_manager::alpha::{AlphaMotionContainer, AlphaMotionType};
pub use super::motion_manager::normal_move::{MoveMotionContainer, MoveMotionType};
pub use super::motion_manager::rotation_move::{RotationMotionContainer, RotationMotionType};
pub use super::motion_manager::s2_move::{ScaleMotionContainer, ScaleMotionType};
pub use super::motion_manager::v3d::{V3dMotionContainer, V3dMotionType};
pub use super::motion_manager::z_move::{ZMotionContainer, ZMotionType};
use super::text_manager::TextManager;
use crate::subsystem::resources::prim::{PrimType, Prim};
use super::parts_manager::PartsManager;
use super::prim::{PrimManager, INVAILD_PRIM_HANDLE};
use anyhow::{bail, Result};
use atomic_refcell::AtomicRefCell;
use std::cell::{Ref, RefMut};
use image::GenericImageView;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DissolveType {
    // no animation
    None = 0,
    Static = 1,
    ColoredFadeIn = 2,
    ColoredFadeOut = 3,
    MaskFadeIn = 4,
    MaskFadeInOut = 5,
    MaskFadeOut = 6,
}

pub struct MotionManager {
    alpha_motion_container: AlphaMotionContainer,
    move_motion_container: MoveMotionContainer,
    rotation_motion_container: RotationMotionContainer,
    scale_motion_container: ScaleMotionContainer,
    z_motion_container: ZMotionContainer,
    v3d_motion_container: V3dMotionContainer,
    snow_motion_container: SnowMotionContainer,
    pub(crate) prim_manager: PrimManager,
    pub(crate) parts_manager: AtomicRefCell<PartsManager>,
    pub(crate) gaiji_manager: GaijiManager,
    textures: Vec<GraphBuff>,
    pub(crate) text_manager: TextManager,
    mask_prim: Prim,
    dissolve_type: DissolveType,
    dissolve_color_id: u32,
    dissolve_mask_graph: GraphBuff,
}

impl Default for MotionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MotionManager {
    pub fn new() -> MotionManager {
        let parts_manager = AtomicRefCell::new(PartsManager::new());

        MotionManager {
            alpha_motion_container: AlphaMotionContainer::new(),
            move_motion_container: MoveMotionContainer::new(),
            rotation_motion_container: RotationMotionContainer::new(),
            scale_motion_container: ScaleMotionContainer::new(),
            z_motion_container: ZMotionContainer::new(),
            v3d_motion_container: V3dMotionContainer::new(),
            snow_motion_container: SnowMotionContainer::new(),
            prim_manager: PrimManager::new(),
            parts_manager,
            textures: vec![GraphBuff::new(); 4096],
            gaiji_manager: GaijiManager::new(),
            text_manager: TextManager::new(),
            mask_prim: Prim::new(),
            dissolve_type: DissolveType::None,
            dissolve_color_id: 0,
            dissolve_mask_graph: GraphBuff::new(),
        }
    }

    pub fn update_alpha_motions(
        &mut self,
        elapsed: i64,
        flag: bool,
    ) {
        self.alpha_motion_container.exec_alpha_motion(&self.prim_manager, flag, elapsed as i32);
    }

    pub fn update_move_motions(
        &mut self,
        elapsed: i64,
        flag: bool,
    ) {
        self.move_motion_container.exec_move_motion(&self.prim_manager, flag, elapsed as i32);
    }

    pub fn update_s2_move_motions(
        &mut self,
        elapsed: i64,
        flag: bool,
    ) {
        self.scale_motion_container.exec_s2_motion(&self.prim_manager, flag, elapsed as i32);
    }

    pub fn update_rotation_motions(
        &mut self,
        elapsed: i64,
        flag: bool,
    ) {
        self.rotation_motion_container.exec_rotation_motion(&self.prim_manager, flag, elapsed as i32);
    }

    pub fn update_z_motions(
        &mut self,
        elapsed: i64,
        flag: bool,
    ) {
        self.z_motion_container.exec_z_motion(&self.prim_manager, flag, elapsed as i32);
    }

    pub fn update_v3d_motions(
        &mut self,
        elapsed: i64,
        flag: bool,
    ) {
        self.v3d_motion_container.exec_v3d_update(&self.prim_manager, flag, elapsed as i32);
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

    #[allow(clippy::too_many_arguments)]
    pub fn set_snow_motion(
        &mut self,
        id: u32,
        a2: i32,
        a3: i32,
        a4: i32,
        a5: i32,
        a6: i32,
        a7: i32,
        a8: i32,
        a9: i32,
        a10: i32,
        a11: i32,
        a12: i32,
        a13: i32,
        a14: i32,
        a15: i32,
        a16: i32,
        a17: i32,
        a18: i32,
        screen_width: u32,
        screen_height: u32,
    ) {
        self.snow_motion_container.push_motion(
            id, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18,
            screen_width, screen_height,
        )
    }

    pub fn start_snow_motion(&mut self, id: u32) {
        self.snow_motion_container.start_snow_motion(id)
    }

    pub fn test_snow_motion(&self, id: u32) -> bool {
        self.snow_motion_container.test_snow_motion(id)
    }

    pub fn stop_snow_motion(&mut self, id: u32) {
        self.snow_motion_container.stop_snow_motion(id)
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
        self.z_motion_container.push_motion(
            prim_id,
            src_z as i16,
            dst_z as i16,
            duration,
            typ,
            reverse,
        )
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

    pub fn set_parts_motion(
        &mut self,
        parts_id: u8,
        entry_id: u8,
        duration: u32,
    ) -> Result<()> {
        self.parts_manager.get_mut().set_motion(
            parts_id,
            entry_id,
            duration
        )
    }

    pub fn stop_parts_motion(&mut self, parts_id: u8) -> Result<()> {
        self.parts_manager.get_mut().stop_motion(parts_id)
    }

    pub fn test_parts_motion(&mut self, parts_id: u8) -> bool {
        self.parts_manager.get_mut().test_motion(parts_id)
    }

    pub fn get_texture(&self, id: u32) -> &GraphBuff {
        &self.textures[id as usize]
    }

    pub fn draw_parts_to_texture(&mut self, parts_id: u8, entry_id: u32) -> Result<()> {
        let parts = self.parts_manager.get_mut().get(parts_id);
        if entry_id >= parts.get_texture_count() {
            bail!("draw_parts_to_texture: invalid entry_id");
        }

        let prim_id = parts.get_prim_id();
        let texture = &mut self.textures[prim_id as usize];
        if !texture.get_texture_ready() {
            bail!("draw_parts_to_texture: texture not ready");
        }

        if texture.get_width() < parts.get_width() + parts.get_offset_x()
            || texture.get_height() < parts.get_height() + parts.get_offset_y()
        {
            bail!("draw_parts_to_texture: invalid texture size");
        }

        let parts_texture = parts.get_texture(entry_id as usize)?;

        if let Some(dest) = texture.get_texture_mut() {
            let src_x = 0;
            let src_y = 0;
            let src_w = parts.get_width() as u32;
            let src_h = parts.get_height() as u32;
            let dest_x = parts.get_offset_x() as u32;
            let dest_y = parts.get_offset_y() as u32;

            if let Err(e) = copy_rect(
                &parts_texture,
                src_x,
                src_y,
                src_w,
                src_h,
                dest,
                dest_x,
                dest_y,
            ) {
                log::warn!("draw_parts_to_texture: {}", e);
            }
        }

        Ok(())
    }


    fn prim_hit_priv(
        &self,
        prim: RefMut<'_, Prim>,
        x: i32,
        y: i32,
        cursor_in: bool,
        cursor_x: i32,
        cursor_y: i32,
    ) -> bool {
        if !cursor_in {
            return false;
        }
        let mut sprite = prim;
        loop {
            if sprite.get_sprt() == INVAILD_PRIM_HANDLE {
                break;
            }
            sprite = self.prim_manager.get_prim(sprite.get_sprt());
        }

        match sprite.get_type() {
            PrimType::PrimTypeGroup => {
                let mut child = sprite.get_child();
                if child != INVAILD_PRIM_HANDLE {
                    loop {
                        let p = self.prim_manager.get_prim(child);
                        if self.prim_hit_priv(p, x, y, cursor_in, cursor_x, cursor_y) {
                            return true;
                        }

                        let p = self.prim_manager.get_prim_immutable(child);
                        child = p.get_grand_son();
                        if child == INVAILD_PRIM_HANDLE {
                            return false;
                        }
                    }
                }
                return false;
            }
            PrimType::PrimTypeTile => {
                if sprite.get_alpha() == 0 {
                    return false;
                }

                let cur_x = x + sprite.get_x() as i32;
                let cur_y = y + sprite.get_y() as i32;
                if cursor_x >= cur_x
                    && cursor_x < cur_x + sprite.get_w() as i32
                    && cursor_y >= cur_y
                    && cursor_y < cur_y + sprite.get_h() as i32
                {
                    return true;
                }
                return false;
            }
            PrimType::PrimTypeSprt => {
                let texture_id = sprite.get_text_index();
                let texture = self.get_texture(texture_id as u32);
                let mut total_x = x + texture.get_offset_x() as i32 + sprite.get_x() as i32;
                let mut total_y = y + texture.get_offset_y() as i32 + sprite.get_y() as i32;
                let mut u = 0i32;
                let mut v = 0i32;
                let mut edge_x = 0i32;
                let mut edge_y = 0i32;
                if sprite.get_attr() & 1 != 0 {
                    let mut sprt_w = sprite.get_w() as i32;
                    if sprt_w > texture.get_width() as i32 {
                        sprt_w = texture.get_width() as i32;
                    }
                    let mut sprt_h = sprite.get_h() as i32;
                    if sprt_h > texture.get_height() as i32 {
                        sprt_h = texture.get_height() as i32;
                    }
                    edge_x = total_x + sprt_w;
                    edge_y = total_y + sprt_h;
                    u = sprite.get_u() as i32;
                    v = sprite.get_v() as i32;
                } else {
                    edge_x = texture.get_width() as i32 + total_x;
                    edge_y = texture.get_height() as i32 + total_y;
                }

                if cursor_x < total_x
                    || cursor_x >= edge_x
                    || cursor_y < total_y
                    || cursor_y >= edge_y
                {
                    return false;
                }

                let adjusted_x = cursor_x + u - total_x;
                let adjusted_y = cursor_y + v - total_y;
                if adjusted_x >= texture.get_width() as i32 || adjusted_y >= texture.get_height() as i32 {
                    return false;
                }

                if let Some(tex) = texture.get_texture() {
                    if adjusted_x >= cursor_x
                        && adjusted_x < texture.get_width() as i32
                        && adjusted_y >= 0
                        && adjusted_y < texture.get_height() as i32
                    {
                        let left = adjusted_x;
                        let top = adjusted_y;
                        let pixel = tex.get_pixel(left as u32, top as u32);
                        let alpha_value = pixel.0[3];
                        if alpha_value != 0 {
                            return true;
                        }
                    }
                }
            }
            _ => return false,
        };

        false
    }

    pub fn prim_hit(
        &self,
        id: i32,
        flag: bool,
        cursor_in: bool,
        cursor_x: i32,
        cursor_y: i32,
    ) -> bool {
        let prim = self.prim_manager.get_prim(id as i16);
        if !flag && !prim.get_draw_flag() {
            return false;
        }

        let mut parent = prim.get_parent();
        let mut x = 0i32;
        let mut y = 0i32;
        if parent != INVAILD_PRIM_HANDLE {
            let mut found = false;
            loop {
                if parent == 0 || parent as u16 == self.prim_manager.get_custom_root_prim_id() {
                    found = true;
                }
                let parent_prim = self.prim_manager.get_prim(parent);
                if !parent_prim.get_draw_flag() {
                    break;
                }
                x += parent_prim.get_x() as i32;
                y += parent_prim.get_y() as i32;
                parent = parent_prim.get_parent();
                if parent == INVAILD_PRIM_HANDLE {
                    if !found {
                        return false;
                    }
                    return self.prim_hit_priv(prim, x, y, cursor_in, cursor_x, cursor_y);
                }
            }
        }

        false
    }

    pub fn load_graph(&mut self, id: u16, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let graph = &mut self.textures[id as usize];
        graph.load_texture(file_name, buff)
    }

    pub fn unload_graph(&mut self, id: u16) {
        let graph = &mut self.textures[id as usize];
        graph.unload();
    }

    pub fn graph_color_tone(&mut self, id: u16, r: i32, g: i32, b: i32) {
        let graph = &mut self.textures[id as usize];
        graph.set_color_tone(r, g, b);
    }

    pub fn refresh_prims(&mut self, graph_id: u16) {
        for prim in self.prim_manager.get_prims_mut().iter_mut().skip(1) {
            let mut prim = prim.borrow_mut();
            if prim.get_type() == PrimType::PrimTypeSprt && prim.get_text_index() as u16 == graph_id {
                prim.apply_attr(0x40);
            }
        }
    }

    pub fn set_gaiji(&mut self, code: char, size: u8, filename: &str, buff: Vec<u8>) -> Result<()> {
        let mut texture = GraphBuff::new();
        texture.load_gaiji_fontface_glyph(filename, buff)?;
        self.gaiji_manager.set_gaiji(code, size, texture);
        Ok(())
    }

    pub fn get_mask_prim(&mut self) -> &mut Prim {
        &mut self.mask_prim
    }

    pub fn set_dissolve_type(&mut self, typ: DissolveType) {
        self.dissolve_type = typ;
    }

    pub fn get_dissolve_type(&self) -> DissolveType {
        self.dissolve_type
    }

    pub fn set_dissolve_color_id(&mut self, color_id: u32) {
        self.dissolve_color_id = color_id;
    }

    pub fn get_dissolve_color_id(&self) -> u32 {
        self.dissolve_color_id
    }

    pub fn set_dissolve_mask_graph(&mut self, graph: GraphBuff) {
        self.dissolve_mask_graph = graph;
    }

    pub fn get_dissolve_mask_graph(&self) -> &GraphBuff {
        &self.dissolve_mask_graph
    }

    pub fn load_texture_from_buff(&mut self, id: u16, buff: Vec<u8>, width: u32, height: u32) -> Result<()> {
        let graph = &mut self.textures[id as usize];
        graph.load_from_buff(buff, width, height)
    }

    pub fn text_reprint(&mut self) {
        todo!()
    }
}
