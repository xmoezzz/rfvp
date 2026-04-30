#![cfg(all(rfvp_switch, feature = "switch-core"))]

use std::collections::HashMap;

use anyhow::Result;
use glam::{Mat4, Vec2, Vec4, vec3};
use image::GenericImageView;
use rfvp_switch_render::{
    ColorF32, FillQuad, Mat4F32, RectF32, RenderCommand, SwitchRenderer,
    TexturedQuad, TextureId,
};

use crate::subsystem::resources::graph_buff::GraphBuff;
use crate::subsystem::resources::motion_manager::MotionManager;
use crate::subsystem::resources::prim::{Prim, PrimManager, PrimType, INVAILD_PRIM_HANDLE};

#[derive(Clone, Copy, Debug)]
struct GraphCacheEntry {
    texture: TextureId,
    generation: u64,
    width: u32,
    height: u32,
}

pub struct SwitchPrimRenderer {
    renderer: SwitchRenderer,
    graph_cache: HashMap<u16, GraphCacheEntry>,
    upload_buffers: Vec<Vec<u8>>,
    virtual_size: (u32, u32),
}

impl SwitchPrimRenderer {
    pub fn new(virtual_size: (u32, u32)) -> Self {
        let mut renderer = SwitchRenderer::new();
        renderer.resize(virtual_size.0, virtual_size.1);
        Self {
            renderer,
            graph_cache: HashMap::new(),
            upload_buffers: Vec::new(),
            virtual_size,
        }
    }

    pub fn rebuild(&mut self, motion: &MotionManager) -> Result<()> {
        self.upload_buffers.clear();
        self.renderer.begin_frame(ColorF32 {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }).map_err(|e| anyhow::anyhow!("Switch render begin_frame failed: {:?}", e))?;

        let prim_manager = motion.prim_manager();
        let graphs = motion.graphs();

        let mut visit = vec![0u8; 4096];
        self.collect_tree(
            prim_manager,
            motion,
            graphs,
            0,
            0.0,
            0.0,
            &mut visit,
            0,
        )?;

        let root = prim_manager.get_custom_root_prim_id() as i16;
        if root != 0 {
            let mut visit = vec![0u8; 4096];
            self.collect_tree(
                prim_manager,
                motion,
                graphs,
                root,
                0.0,
                0.0,
                &mut visit,
                0,
            )?;
        }

        self.renderer.end_frame().map_err(|e| anyhow::anyhow!("Switch render end_frame failed: {:?}", e))?;
        Ok(())
    }

    pub fn commands(&self) -> &[RenderCommand] {
        self.renderer.commands()
    }

    fn collect_tree(
        &mut self,
        prim_manager: &PrimManager,
        motion: &MotionManager,
        graphs: &[GraphBuff],
        prim_id: i16,
        parent_x: f32,
        parent_y: f32,
        visit: &mut [u8],
        depth: usize,
    ) -> Result<()> {
        if prim_id < 0 {
            return Ok(());
        }
        let prim_idx = prim_id as usize;
        if prim_idx >= visit.len() || depth > 4096 {
            return Ok(());
        }
        if visit[prim_idx] != 0 {
            return Ok(());
        }
        visit[prim_idx] = 1;

        let base_prim = prim_manager.get_prim_immutable(prim_id);
        if !base_prim.get_draw_flag() {
            visit[prim_idx] = 2;
            return Ok(());
        }

        let draw_x = base_prim.get_x() as f32;
        let draw_y = base_prim.get_y() as f32;
        let draw_alpha = base_prim.get_alpha() as f32 / 255.0;

        let mut draw_id = prim_id;
        let mut sprt = base_prim.get_sprt();
        while sprt != INVAILD_PRIM_HANDLE {
            if sprt < 0 || sprt >= 4096 {
                visit[prim_idx] = 2;
                return Ok(());
            }
            let sref = prim_manager.get_prim_immutable(sprt);
            if !sref.get_draw_flag() {
                visit[prim_idx] = 2;
                return Ok(());
            }
            draw_id = sprt;
            sprt = sref.get_sprt();
        }
        drop(base_prim);

        let draw_prim = prim_manager.get_prim_immutable(draw_id);
        let draw_type = draw_prim.get_type();
        let first_child = draw_prim.get_first_child_idx();

        match draw_type {
            PrimType::PrimTypeGroup | PrimType::PrimTypeNone => {}
            PrimType::PrimTypeSprt => {
                self.emit_sprite(
                    &draw_prim,
                    graphs,
                    draw_id,
                    parent_x,
                    parent_y,
                    draw_x,
                    draw_y,
                    draw_alpha,
                    motion,
                )?;
            }
            PrimType::PrimTypeText => {
                self.emit_text(
                    &draw_prim,
                    graphs,
                    parent_x,
                    parent_y,
                    draw_x,
                    draw_y,
                    draw_alpha,
                    motion,
                )?;
            }
            PrimType::PrimTypeTile => {
                self.emit_tile(
                    &draw_prim,
                    motion,
                    parent_x,
                    parent_y,
                    draw_x,
                    draw_y,
                    draw_alpha,
                )?;
            }
            PrimType::PrimTypeSnow => {}
        }

        let mut children = Vec::new();
        let mut child = first_child;
        let mut steps = 0usize;
        while child != INVAILD_PRIM_HANDLE {
            if steps >= 4096 || child < 0 || child >= 4096 {
                break;
            }
            steps += 1;
            children.push(child);
            let p = prim_manager.get_prim_immutable(child);
            child = p.get_next_sibling_idx();
        }
        drop(draw_prim);

        let next_parent_x = parent_x + draw_x;
        let next_parent_y = parent_y + draw_y;
        for cid in children {
            self.collect_tree(
                prim_manager,
                motion,
                graphs,
                cid,
                next_parent_x,
                next_parent_y,
                visit,
                depth + 1,
            )?;
        }

        visit[prim_idx] = 2;
        Ok(())
    }

    fn emit_sprite(
        &mut self,
        prim: &Prim,
        graphs: &[GraphBuff],
        _draw_id: i16,
        parent_x: f32,
        parent_y: f32,
        draw_x: f32,
        draw_y: f32,
        draw_alpha: f32,
        motion: &MotionManager,
    ) -> Result<()> {
        let tex_id = prim.get_texture_id();
        let graph_id = if tex_id >= 0 {
            Some(tex_id as u16)
        } else if tex_id == -2 {
            Some(crate::subsystem::resources::videoplayer::MOVIE_GRAPH_ID)
        } else {
            None
        };
        let Some(graph_id) = graph_id else {
            return Ok(());
        };
        let Some(g) = graphs.get(graph_id as usize) else {
            return Ok(());
        };
        let Some(texture) = self.upload_graph_if_needed(graph_id, g)? else {
            return Ok(());
        };
        let Some(img) = g.get_texture().as_ref() else {
            return Ok(());
        };
        let (tw, th) = img.dimensions();
        if tw == 0 || th == 0 {
            return Ok(());
        }

        let attr = prim.get_attr();
        let use_rect = (attr & 1) != 0;
        let (w, h, u, v) = if use_rect {
            let mut w = prim.get_w() as f32;
            let mut h = prim.get_h() as f32;
            if w <= 0.0 {
                w = g.get_width() as f32;
            }
            if h <= 0.0 {
                h = g.get_height() as f32;
            }
            (
                w.min(g.get_width() as f32),
                h.min(g.get_height() as f32),
                prim.get_u() as f32,
                prim.get_v() as f32,
            )
        } else {
            (g.get_width() as f32, g.get_height() as f32, 0.0, 0.0)
        };
        let uv0 = Vec2::new(u / tw as f32, v / th as f32);
        let uv1 = Vec2::new((u + w) / tw as f32, (v + h) / th as f32);
        let (pivot_x, pivot_y) = if (attr & 2) != 0 {
            (prim.get_opx() as f32, prim.get_opy() as f32)
        } else {
            (g.get_u() as f32, g.get_v() as f32)
        };
        let model = self.build_draw_model(
            prim,
            parent_x,
            parent_y,
            draw_x,
            draw_y,
            g.get_offset_x() as f32,
            g.get_offset_y() as f32,
            pivot_x,
            pivot_y,
            motion,
        );
        self.draw_textured(texture, model, w, h, uv0, uv1, Vec4::new(1.0, 1.0, 1.0, draw_alpha))
    }

    fn emit_text(
        &mut self,
        prim: &Prim,
        graphs: &[GraphBuff],
        parent_x: f32,
        parent_y: f32,
        draw_x: f32,
        draw_y: f32,
        draw_alpha: f32,
        motion: &MotionManager,
    ) -> Result<()> {
        let slot = prim.get_text_index();
        if !(0..=31).contains(&slot) {
            return Ok(());
        }
        let graph_id = 4064u16 + slot as u16;
        let Some(g) = graphs.get(graph_id as usize) else {
            return Ok(());
        };
        let Some(texture) = self.upload_graph_if_needed(graph_id, g)? else {
            return Ok(());
        };
        let Some(img) = g.get_texture().as_ref() else {
            return Ok(());
        };
        let (tw, th) = img.dimensions();
        if tw == 0 || th == 0 {
            return Ok(());
        }

        let attr = prim.get_attr();
        let use_rect = (attr & 1) != 0;
        let display_w = g.get_display_width() as f32;
        let display_h = g.get_display_height() as f32;
        let tex_scale_x = if display_w > 0.0 { tw as f32 / display_w } else { 1.0 };
        let tex_scale_y = if display_h > 0.0 { th as f32 / display_h } else { 1.0 };
        let (w, h, u, v, tex_w, tex_h) = if use_rect {
            let mut w = prim.get_w() as f32;
            let mut h = prim.get_h() as f32;
            if w <= 0.0 {
                w = display_w;
            }
            if h <= 0.0 {
                h = display_h;
            }
            let w = w.min(display_w);
            let h = h.min(display_h);
            let u = prim.get_u() as f32;
            let v = prim.get_v() as f32;
            (w, h, u, v, w * tex_scale_x, h * tex_scale_y)
        } else {
            (display_w, display_h, 0.0, 0.0, tw as f32, th as f32)
        };
        let tex_u = u * tex_scale_x;
        let tex_v = v * tex_scale_y;
        let uv0 = Vec2::new(tex_u / tw as f32, tex_v / th as f32);
        let uv1 = Vec2::new((tex_u + tex_w) / tw as f32, (tex_v + tex_h) / th as f32);
        let (pivot_x, pivot_y) = if (attr & 2) != 0 {
            (prim.get_opx() as f32, prim.get_opy() as f32)
        } else {
            (g.get_u() as f32, g.get_v() as f32)
        };
        let model = self.build_draw_model(
            prim,
            parent_x,
            parent_y,
            draw_x,
            draw_y,
            g.get_offset_x() as f32,
            g.get_offset_y() as f32,
            pivot_x,
            pivot_y,
            motion,
        );
        self.draw_textured(texture, model, w, h, uv0, uv1, Vec4::new(1.0, 1.0, 1.0, draw_alpha))
    }

    fn emit_tile(
        &mut self,
        prim: &Prim,
        motion: &MotionManager,
        parent_x: f32,
        parent_y: f32,
        draw_x: f32,
        draw_y: f32,
        draw_alpha: f32,
    ) -> Result<()> {
        let w = prim.get_w() as f32;
        let h = prim.get_h() as f32;
        if w <= 0.0 || h <= 0.0 {
            return Ok(());
        }
        let c = motion.color_manager.get_entry(prim.get_tile() as u8);
        let color = ColorF32 {
            r: c.get_r() as f32 / 255.0,
            g: c.get_g() as f32 / 255.0,
            b: c.get_b() as f32 / 255.0,
            a: draw_alpha * (c.get_a() as f32 / 255.0),
        };
        let (pivot_x, pivot_y) = if (prim.get_attr() & 2) != 0 {
            (prim.get_opx() as f32, prim.get_opy() as f32)
        } else {
            (0.0, 0.0)
        };
        let model = self.build_draw_model(
            prim,
            parent_x,
            parent_y,
            draw_x,
            draw_y,
            0.0,
            0.0,
            pivot_x,
            pivot_y,
            motion,
        );
        self.renderer.draw_fill_quad(FillQuad {
            dst: RectF32 { x: 0.0, y: 0.0, w, h },
            color,
            transform: mat4_to_switch(model),
        }).map_err(|e| anyhow::anyhow!("Switch render draw_fill_quad failed: {:?}", e))?;
        Ok(())
    }

    fn draw_textured(
        &mut self,
        texture: TextureId,
        model: Mat4,
        w: f32,
        h: f32,
        uv0: Vec2,
        uv1: Vec2,
        color: Vec4,
    ) -> Result<()> {
        self.renderer.draw_textured_quad(TexturedQuad {
            texture,
            dst: RectF32 { x: 0.0, y: 0.0, w, h },
            uv: RectF32 {
                x: uv0.x,
                y: uv0.y,
                w: uv1.x - uv0.x,
                h: uv1.y - uv0.y,
            },
            color: ColorF32 {
                r: color.x,
                g: color.y,
                b: color.z,
                a: color.w,
            },
            transform: mat4_to_switch(model),
        }).map_err(|e| anyhow::anyhow!("Switch render draw_textured_quad failed: {:?}", e))?;
        Ok(())
    }

    fn upload_graph_if_needed(&mut self, graph_id: u16, graph: &GraphBuff) -> Result<Option<TextureId>> {
        let Some(img) = graph.get_texture().as_ref() else {
            return Ok(None);
        };
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let generation = graph.get_generation();
        if let Some(entry) = self.graph_cache.get(&graph_id) {
            if entry.generation == generation && entry.width == width && entry.height == height {
                return Ok(Some(entry.texture));
            }
        }

        let rgba = img.to_rgba8().into_raw();
        self.upload_buffers.push(rgba);
        let data = self.upload_buffers.last().unwrap();
        let ptr = data.as_ptr();
        let len = data.len();

        let texture = if let Some(entry) = self.graph_cache.get(&graph_id).copied() {
            self.renderer.upload_texture_rgba8(entry.texture, width, height, ptr, len, generation).map_err(|e| anyhow::anyhow!("Switch render upload_texture_rgba8 failed: {:?}", e))?;
            entry.texture
        } else {
            self.renderer.register_texture_rgba8_with_pixels(width, height, ptr, len, generation).map_err(|e| anyhow::anyhow!("Switch render register_texture_rgba8 failed: {:?}", e))?
        };

        self.graph_cache.insert(graph_id, GraphCacheEntry {
            texture,
            generation,
            width,
            height,
        });
        Ok(Some(texture))
    }

    fn build_draw_model(
        &self,
        prim: &Prim,
        parent_x: f32,
        parent_y: f32,
        draw_x: f32,
        draw_y: f32,
        off_x: f32,
        off_y: f32,
        pivot_x: f32,
        pivot_y: f32,
        motion: &MotionManager,
    ) -> Mat4 {
        let theta = -(prim.get_angle() as f32) * std::f32::consts::TAU / 3600.0;
        let attr = prim.get_attr();
        let pos_x = parent_x + draw_x;
        let pos_y = parent_y + draw_y;
        let local_x = off_x - pivot_x;
        let local_y = off_y - pivot_y;

        if (attr & 4) != 0 {
            let center_x = self.virtual_size.0 as f32 * 0.5;
            let center_y = self.virtual_size.1 as f32 * 0.5;
            let mut fx = prim.get_factor_x() as f32;
            let mut fy = prim.get_factor_y() as f32;
            if fx.abs() < 1e-6 {
                fx = 1.0;
            }
            if fy.abs() < 1e-6 {
                fy = 1.0;
            }
            let mut depth = prim.get_z() as f32 - motion.get_v3d_z() as f32;
            if depth.abs() < 1e-3 {
                depth = 1.0;
            }
            let sx = fx / depth;
            let sy = fy / depth;
            let cam_shift_x = 1000.0 * motion.get_v3d_x() as f32 / fx;
            let cam_shift_y = 1000.0 * motion.get_v3d_y() as f32 / fy;

            Mat4::from_translation(vec3(center_x, center_y, 0.0))
                * Mat4::from_scale(vec3(sx, sy, 1.0))
                * Mat4::from_translation(vec3(pos_x - cam_shift_x, pos_y - cam_shift_y, 0.0))
                * Mat4::from_rotation_z(theta)
                * Mat4::from_translation(vec3(local_x, local_y, 0.0))
        } else {
            let sx = prim.get_factor_x() as f32 / 1000.0;
            let sy = prim.get_factor_y() as f32 / 1000.0;

            Mat4::from_translation(vec3(pos_x + pivot_x, pos_y + pivot_y, 0.0))
                * Mat4::from_scale(vec3(sx, sy, 1.0))
                * Mat4::from_rotation_z(theta)
                * Mat4::from_translation(vec3(local_x, local_y, 0.0))
        }
    }
}

fn mat4_to_switch(m: Mat4) -> Mat4F32 {
    Mat4F32 {
        cols: m.to_cols_array_2d(),
    }
}
