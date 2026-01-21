mod alpha;
mod normal_move;
mod rotation_move;
mod s2_move;
mod v3d;
mod z_move;
pub(crate) mod snow;
mod anim;
mod dissolve2;

use self::snow::SnowMotionContainer;
use self::anim::SpriteAnimContainer;
use self::dissolve2::Dissolve2State;

use super::gaiji_manager::GaijiManager;
use super::graph_buff::{copy_rect, copy_rect_clipped, GraphBuff};
pub use super::motion_manager::alpha::{AlphaMotionContainer, AlphaMotionType};
pub use super::motion_manager::normal_move::{MoveMotionContainer, MoveMotionType};
pub use super::motion_manager::rotation_move::{RotationMotionContainer, RotationMotionType};
pub use super::motion_manager::s2_move::{ScaleMotionContainer, ScaleMotionType};
pub use super::motion_manager::v3d::{V3dMotionContainer, V3dMotionType};
pub use super::motion_manager::z_move::{ZMotionContainer, ZMotionType};
use super::text_manager::TextManager;
use crate::subsystem::resources::color_manager::ColorManager;
use crate::subsystem::resources::prim::{PrimType, Prim};
use super::parts_manager::PartsManager;
use super::prim::{PrimManager, INVAILD_PRIM_HANDLE};
use anyhow::{bail, Result};
use atomic_refcell::{AtomicRefCell, AtomicRefMut};
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
    sprite_anim_container: SpriteAnimContainer,
    pub(crate) color_manager: ColorManager,
    pub(crate) prim_manager: PrimManager,
    pub(crate) parts_manager: AtomicRefCell<PartsManager>,
    pub(crate) gaiji_manager: GaijiManager,
    textures: Vec<GraphBuff>,
    pub(crate) text_manager: TextManager,
    mask_prim: Prim,
    dissolve_type: DissolveType,
    dissolve_color_id: u32,
    dissolve_mask_graph: GraphBuff,
    dissolve_duration_ms: u32,
    dissolve_elapsed_ms: u32,
    dissolve_alpha: f32,
    dissolve2: Dissolve2State,
}

impl Default for MotionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MotionManager {

/// Read-only access for renderers.
pub fn prim_manager(&self) -> &PrimManager {
    &self.prim_manager
}

/// Read-only access for renderers.
pub fn graphs(&self) -> &[GraphBuff] {
    &self.textures
}

/// Read-only access for snow renderer.
pub(crate) fn snow_motions(&self) -> &[snow::SnowMotion] {
    self.snow_motion_container.motions()
}

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
            sprite_anim_container: SpriteAnimContainer::new(),
            prim_manager: PrimManager::new(),
            color_manager: ColorManager::new(),
            parts_manager,
            textures: vec![GraphBuff::new(); 4096],
            gaiji_manager: GaijiManager::new(),
            text_manager: TextManager::new(),
            mask_prim: Prim::new(),
            dissolve_type: DissolveType::None,
            dissolve_color_id: 0,
            dissolve_mask_graph: GraphBuff::new(),
            dissolve_duration_ms: 0,
            dissolve_elapsed_ms: 0,
            dissolve_alpha: 0.0,
            dissolve2: Dissolve2State::new(),
        }
    }

    pub fn update_anim_motions(&mut self, elapsed: i64) {
        self.sprite_anim_container.update(&self.prim_manager, elapsed as i32);
    }

    /// Advance PartsMotion timers and apply completed entries to their destination primitives.
    pub fn update_parts_motions(&mut self, elapsed: i64) {
        // The original engine passes a *negative* elapsed in "Ctrl/ControlPulse" fast-forward
        // mode. For PartsMotion, a negative elapsed means: skip interpolation and commit the
        // final bitmap immediately.
        if elapsed == 0 {
            return;
        }

        let elapsed_ms: u32 = if elapsed < 0 { u32::MAX } else { elapsed as u32 };

        let mut completed: Vec<(u8, u8)> = Vec::new();
        {
            let mut pm = self.parts_manager.borrow_mut();
            pm.tick_motions(elapsed_ms, &mut completed);
        }

        for (parts_id, entry_id) in completed {
            if let Err(e) = self.draw_parts_to_texture(parts_id, entry_id as u32) {
                log::warn!("update_parts_motions: failed to apply parts_id={} entry_id={}: {}", parts_id, entry_id, e);
            }
        }
    }

    pub fn update_snow_motions(&mut self, elapsed: i64, screen_w: i32, screen_h: i32) {
        self.snow_motion_container.exec_snow_motion(elapsed as i32, screen_w, screen_h);
    }

    pub fn set_anim_motion(&mut self, prim_id: u32, base_graph_id: i32, start: i32, end: i32) -> Result<()> {
        self.sprite_anim_container.set_motion(prim_id, base_graph_id, start, end)
    }

    pub fn stop_anim_motion(&mut self, prim_id: u32) -> Result<()> {
        self.sprite_anim_container.stop_motion(prim_id)
    }

    pub fn test_anim_motion(&self, prim_id: u32) -> bool {
        self.sprite_anim_container.test_motion(prim_id)
    }

    /// Tick reveal-by-time for all text slots and upload dirty slots.
    pub fn update_text_reveal(&mut self, elapsed: i64, fonts: &crate::subsystem::resources::text_manager::FontEnumerator) {
        // In the original engine, holding Ctrl (or issuing ControlPulse) forces text reveal
        // to complete immediately for the current frame.
        if elapsed < 0 {
            self.text_manager.force_reveal_all_non_suspended();
        } else {
            self.text_manager.tick(elapsed as u32);
        }
        self.text_reprint(fonts);
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
            screen_width as i32, screen_height as i32,
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

        // Signed offsets (negative offsets are allowed and handled via clipping).
        let dx = parts.get_offset_x_i16() as i32;
        let dy = parts.get_offset_y_i16() as i32;
        let end_x = dx + parts.get_width() as i32;
        let end_y = dy + parts.get_height() as i32;

        if end_x > texture.get_width() as i32 || end_y > texture.get_height() as i32 {
            bail!("draw_parts_to_texture: invalid texture size");
        }

        let parts_texture = parts.get_texture(entry_id as usize)?;

        if let Some(dest) = texture.get_texture_mut().as_mut() {
            let src_x = 0;
            let src_y = 0;
            let src_w = parts.get_width() as u32;
            let src_h = parts.get_height() as u32;

            if let Err(e) = copy_rect_clipped(
                &parts_texture,
                src_x,
                src_y,
                src_w,
                src_h,
                dest,
                dx,
                dy,
            ) {
                log::warn!("draw_parts_to_texture: {}", e);
            }
        }

        // The destination GraphBuff pixels changed; bump generation so GPU cache can refresh.
        texture.mark_dirty();

        Ok(())
    }


    fn prim_hit_priv(
        &self,
        prim: AtomicRefMut<'_, Prim>,
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
                let mut child = sprite.get_first_child_idx();
                if child != INVAILD_PRIM_HANDLE {
                    loop {
                        let p = self.prim_manager.get_prim(child);
                        if self.prim_hit_priv(p, x, y, cursor_in, cursor_x, cursor_y) {
                            return true;
                        }

                        let p = self.prim_manager.get_prim_immutable(child);
                        child = p.get_next_sibling_idx();
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
                let total_x = x + texture.get_offset_x() as i32 + sprite.get_x() as i32;
                let total_y = y + texture.get_offset_y() as i32 + sprite.get_y() as i32;
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
            match prim.get_type() {
                PrimType::PrimTypeSprt => {
                    // Sprite: texture_id is a graph id.
                    if prim.get_texture_id() as u16 == graph_id {
                        prim.apply_attr(0x40);
                    }
                }
                PrimType::PrimTypeText => {
                    // Text: text_index is a slot (0..31) mapped to Graph(4064 + slot).
                    let slot = prim.get_text_index();
                    if (0..32).contains(&slot) {
                        let gid = 4064u16 + slot as u16;
                        if gid == graph_id {
                            prim.apply_attr(0x40);
                        }
                    }
                }
                _ => {}
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

    pub fn start_dissolve(&mut self, duration_ms: u32, typ: DissolveType) {
        self.dissolve_duration_ms = duration_ms.max(1);
        self.dissolve_elapsed_ms = 0;
        self.dissolve_type = typ;
        self.dissolve_alpha = 0.0;
    }

    pub fn tick_dissolve(&mut self, elapsed_ms: u32) {
        crate::trace::motion(format_args!("tick_dissolve: elapsed_ms={}", elapsed_ms));
        let typ = self.dissolve_type;
        if typ == DissolveType::None || typ == DissolveType::Static {
            self.dissolve_alpha = if typ == DissolveType::Static { 1.0 } else { 0.0 };
            return;
        }

        self.dissolve_elapsed_ms = self.dissolve_elapsed_ms.saturating_add(elapsed_ms);
        let dur = self.dissolve_duration_ms.max(1);
        let t = (self.dissolve_elapsed_ms as f32 / dur as f32).clamp(0.0, 1.0);

        self.dissolve_alpha = match typ {
            DissolveType::ColoredFadeIn | DissolveType::MaskFadeIn => 1.0 - t,
            DissolveType::ColoredFadeOut | DissolveType::MaskFadeOut => t,
            DissolveType::MaskFadeInOut => 1.0 - (2.0 * t - 1.0).abs(),
            _ => t,
        };

        if self.dissolve_elapsed_ms >= dur {
            match typ {
                DissolveType::ColoredFadeIn | DissolveType::MaskFadeIn | DissolveType::MaskFadeInOut => {
                    self.dissolve_type = DissolveType::None;
                    self.dissolve_alpha = 0.0;
                }
                DissolveType::ColoredFadeOut | DissolveType::MaskFadeOut => {
                    self.dissolve_type = DissolveType::Static;
                    self.dissolve_alpha = 1.0;
                }
                _ => {
                    self.dissolve_type = DissolveType::None;
                    self.dissolve_alpha = 0.0;
                }
            }
        }
    }

    pub fn get_dissolve_alpha(&self) -> f32 {
        self.dissolve_alpha
    }

    // -----------------------------
    // Dissolve2 (engine-internal overlay fade)
    // -----------------------------
    pub fn start_dissolve2_hold(&mut self, color_id: u32) {
        self.dissolve2.start_hold(color_id);
    }

    pub fn start_dissolve2_fade_in(&mut self, color_id: u32, duration_ms: u32) {
        self.dissolve2.start_fade_in(color_id, duration_ms);
    }

    pub fn start_dissolve2_fade_out(&mut self, duration_ms: u32) {
        self.dissolve2.start_fade_out(duration_ms);
    }

    pub fn start_dissolve2_in_out(&mut self, color_id: u32, duration_ms: u32) {
        self.dissolve2.start_in_out(color_id, duration_ms);
    }

    pub fn tick_dissolve2(&mut self, elapsed_ms: u32) {
        self.dissolve2.tick(elapsed_ms);
    }

    pub fn get_dissolve2_alpha(&self) -> f32 {
        self.dissolve2.alpha()
    }

    pub fn get_dissolve2_color_id(&self) -> u32 {
        self.dissolve2.color_id()
    }

    pub fn get_dissolve2_mode(&self) -> u8 {
        self.dissolve2.mode()
    }

    pub fn is_dissolve2_transitioning(&self) -> bool {
        self.dissolve2.is_transitioning()
    }

    pub fn load_texture_from_buff(&mut self, id: u16, buff: Vec<u8>, width: u32, height: u32) -> Result<()> {
        let graph = &mut self.textures[id as usize];
        graph.load_from_buff(buff, width, height)
    }

    pub fn text_reprint(&mut self, fonts: &crate::subsystem::resources::text_manager::FontEnumerator) {
        self.text_reprint_impl(fonts, false);
    }

    pub fn text_reprint_force(&mut self, fonts: &crate::subsystem::resources::text_manager::FontEnumerator) {
        self.text_reprint_impl(fonts, true);
    }

    fn text_reprint_impl(&mut self, fonts: &crate::subsystem::resources::text_manager::FontEnumerator, force: bool) {
        for slot in 0..32 {
            let _ = self.text_upload_slot(slot, fonts, force);
        }
    }

    pub fn text_upload_slot(
        &mut self,
        slot: i32,
        fonts: &crate::subsystem::resources::text_manager::FontEnumerator,
        force_render: bool,
    ) -> anyhow::Result<()> {
        if !(0..32).contains(&slot) {
            return Ok(());
        }
        let graph_id: u16 = 4064u16 + slot as u16;
        if let Some((rgba, w, h)) = self.text_manager.build_slot_rgba(slot, fonts, force_render)? {
            self.load_texture_from_buff(graph_id, rgba, w, h)?;
            self.refresh_prims(graph_id);
        }
        Ok(())
    }
}


impl MotionManager {
    /// Dump motion-related state for debugging (counts and a small sample of running motions).
    pub fn debug_dump_motion_state(&self, max_each: usize) -> String {
        let mut out = String::new();

        out.push_str(&format!(
            "Dissolve: type={:?} elapsed_ms={} dur_ms={} alpha={:.3} color_id={}\n",
            self.dissolve_type,
            self.dissolve_elapsed_ms,
            self.dissolve_duration_ms,
            self.dissolve_alpha,
            self.dissolve_color_id
        ));

        out.push_str(&format!(
            "Dissolve2: mode={} elapsed_ms=? dur_ms=? alpha={:.3} color_id={} transitioning={}\n",
            self.get_dissolve2_mode(),
            self.get_dissolve2_alpha(),
            self.get_dissolve2_color_id(),
            self.is_dissolve2_transitioning()
        ));

        out.push_str(&format!(
            "Running counts: alpha={} move={} rot={} scale={} z={} anim={} snow={}\n",
            self.alpha_motion_container.debug_running_count(),
            self.move_motion_container.debug_running_count(),
            self.rotation_motion_container.debug_running_count(),
            self.scale_motion_container.debug_running_count(),
            self.z_motion_container.debug_running_count(),
            self.sprite_anim_container.debug_running_count(),
            self.snow_motion_container.debug_enabled_count()
        ));

        out.push_str(&self.v3d_motion_container.debug_dump());
        out.push_str(&self.alpha_motion_container.debug_dump(max_each));
        out.push_str(&self.move_motion_container.debug_dump(max_each));
        out.push_str(&self.rotation_motion_container.debug_dump(max_each));
        out.push_str(&self.scale_motion_container.debug_dump(max_each));
        out.push_str(&self.z_motion_container.debug_dump(max_each));
        out.push_str(&self.sprite_anim_container.debug_dump(max_each));
        out.push_str(&self.snow_motion_container.debug_dump(2));
        out
    }

    pub fn debug_dump_prim_tree(&self, max_nodes: usize, max_depth: usize) -> String {
        let root = self.prim_manager.get_custom_root_prim_id() as i16;
        self.prim_manager.debug_dump_tree(root, max_nodes, max_depth)
    }
}
