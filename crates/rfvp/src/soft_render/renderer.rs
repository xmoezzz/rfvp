use glam::{vec2, vec3, vec4, Mat4, Vec2, Vec4};
use image::{DynamicImage, GenericImageView};

use super::{PixelFormat, SoftFramebuffer, SoftRenderError};
use crate::subsystem::resources::{
    color_manager::ColorManager,
    graph_buff::{GraphBuff, GraphBuffLoadKind},
    motion_manager::{snow::SnowMotion, DissolveType, MotionManager},
    prim::{Prim, PrimManager, PrimType},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SoftRendererStats {
    pub quad_count: usize,
    pub draw_calls: usize,
}

#[derive(Debug)]
pub struct SoftRenderer {
    virtual_size: (u32, u32),
    framebuffer: SoftFramebuffer,
    stats: SoftRendererStats,
}

#[derive(Debug, Clone, Copy)]
struct Vertex {
    pos: Vec2,
    uv: Vec2,
    color: Vec4,
}

#[derive(Debug, Clone, Copy)]
enum TextureRef<'a> {
    Graph {
        graph_id: u16,
        graph: &'a GraphBuff,
        image: &'a DynamicImage,
    },
    White,
}

impl SoftRenderer {
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Result<Self, SoftRenderError> {
        Ok(Self {
            virtual_size: (width.max(1), height.max(1)),
            framebuffer: SoftFramebuffer::new(width.max(1), height.max(1), format)?,
            stats: SoftRendererStats::default(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), SoftRenderError> {
        self.virtual_size = (width.max(1), height.max(1));
        self.framebuffer.resize(width.max(1), height.max(1))
    }

    pub fn framebuffer(&self) -> &SoftFramebuffer {
        &self.framebuffer
    }

    pub fn framebuffer_mut(&mut self) -> &mut SoftFramebuffer {
        &mut self.framebuffer
    }

    pub fn stats(&self) -> SoftRendererStats {
        self.stats
    }

    pub fn render_frame(
        &mut self,
        motion: &MotionManager,
    ) -> Result<&SoftFramebuffer, SoftRenderError> {
        self.render_motion(motion)?;
        Ok(self.framebuffer())
    }

    pub fn render_motion(&mut self, motion: &MotionManager) -> Result<(), SoftRenderError> {
        self.stats = SoftRendererStats::default();
        self.framebuffer.clear_rgba(0, 0, 0, 255);

        let prim_manager = motion.prim_manager();
        let graphs = motion.graphs();
        let snow_motions = motion.snow_motions();

        let mut visit = vec![0u8; 4096];
        self.render_tree(
            prim_manager,
            &motion.color_manager,
            graphs,
            snow_motions,
            motion.get_v3d_x(),
            motion.get_v3d_y(),
            motion.get_v3d_z(),
            0,
            0.0,
            0.0,
            &mut visit,
            0,
        )?;

        if let Some(color) = self.dissolve_color(motion)? {
            self.fill_rect(
                0.0,
                0.0,
                self.virtual_size.0 as f32,
                self.virtual_size.1 as f32,
                color,
            );
        }

        let alpha2 = motion.get_dissolve2_alpha();
        if alpha2 > 0.0 {
            let cid = motion.get_dissolve2_color_id() as u8;
            let c = motion.color_manager.get_entry(cid);
            self.fill_rect(
                0.0,
                0.0,
                self.virtual_size.0 as f32,
                self.virtual_size.1 as f32,
                vec4(
                    c.get_r() as f32 / 255.0,
                    c.get_g() as f32 / 255.0,
                    c.get_b() as f32 / 255.0,
                    (c.get_a() as f32 / 255.0) * alpha2,
                ),
            );
        }

        let root = prim_manager.get_custom_root_prim_id() as i16;
        if root != 0 {
            let mut visit = vec![0u8; 4096];
            self.render_tree(
                prim_manager,
                &motion.color_manager,
                graphs,
                snow_motions,
                motion.get_v3d_x(),
                motion.get_v3d_y(),
                motion.get_v3d_z(),
                root,
                0.0,
                0.0,
                &mut visit,
                0,
            )?;
        }

        Ok(())
    }

    fn dissolve_color(&self, motion: &MotionManager) -> Result<Option<Vec4>, SoftRenderError> {
        match motion.get_dissolve_type() {
            DissolveType::None => Ok(None),
            DissolveType::MaskFadeIn | DissolveType::MaskFadeInOut | DissolveType::MaskFadeOut => {
                // Match app.rs: mask dissolves do not enqueue a dedicated overlay in the
                // current wgpu path; root and overlay primitives continue through the normal
                // sprite/fill pipeline order.
                Ok(None)
            }
            DissolveType::Static | DissolveType::ColoredFadeIn | DissolveType::ColoredFadeOut => {
                let alpha = motion.get_dissolve_alpha();
                if alpha <= 0.0 {
                    return Ok(None);
                }
                let cid = motion.get_dissolve_color_id() as u8;
                let c = motion.color_manager.get_entry(cid);
                Ok(Some(vec4(
                    c.get_r() as f32 / 255.0,
                    c.get_g() as f32 / 255.0,
                    c.get_b() as f32 / 255.0,
                    (c.get_a() as f32 / 255.0) * alpha,
                )))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_tree(
        &mut self,
        prim_manager: &PrimManager,
        color_manager: &ColorManager,
        graphs: &[GraphBuff],
        snow_motions: &[SnowMotion],
        v3d_x: i32,
        v3d_y: i32,
        v3d_z: i32,
        prim_id: i16,
        parent_x: f32,
        parent_y: f32,
        visit: &mut [u8],
        depth: usize,
    ) -> Result<(), SoftRenderError> {
        if prim_id < 0 {
            return Ok(());
        }
        let prim_idx = prim_id as usize;
        if prim_idx >= visit.len() || depth > 4096 || visit[prim_idx] != 0 {
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
        while sprt != -1 {
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

        let draw_prim = prim_manager.get_prim_immutable(draw_id);
        let first_child = draw_prim.get_first_child_idx();

        match draw_prim.get_type() {
            PrimType::PrimTypeGroup | PrimType::PrimTypeNone => {}
            PrimType::PrimTypeSprt => self.draw_sprite_prim(
                &draw_prim, graphs, parent_x, parent_y, draw_x, draw_y, draw_alpha, v3d_x, v3d_y,
                v3d_z,
            )?,
            PrimType::PrimTypeText => self.draw_text_prim(
                &draw_prim, graphs, parent_x, parent_y, draw_x, draw_y, draw_alpha, v3d_x, v3d_y,
                v3d_z,
            )?,
            PrimType::PrimTypeSnow => self.draw_snow_prim(
                &draw_prim,
                graphs,
                snow_motions,
                parent_x,
                parent_y,
                draw_x,
                draw_y,
                draw_alpha,
            )?,
            PrimType::PrimTypeTile => self.draw_tile_prim(
                &draw_prim,
                color_manager,
                parent_x,
                parent_y,
                draw_x,
                draw_y,
                draw_alpha,
                v3d_x,
                v3d_y,
                v3d_z,
            ),
        }

        let mut children = Vec::new();
        let mut child = first_child;
        let mut steps = 0usize;
        while child != -1 && steps < 4096 {
            steps += 1;
            if child < 0 || child >= 4096 {
                break;
            }
            children.push(child);
            let p = prim_manager.get_prim_immutable(child);
            child = p.get_next_sibling_idx();
        }

        let next_parent_x = parent_x + draw_x;
        let next_parent_y = parent_y + draw_y;
        drop(draw_prim);
        drop(base_prim);

        for cid in children {
            self.render_tree(
                prim_manager,
                color_manager,
                graphs,
                snow_motions,
                v3d_x,
                v3d_y,
                v3d_z,
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

    #[allow(clippy::too_many_arguments)]
    fn draw_sprite_prim(
        &mut self,
        prim: &Prim,
        graphs: &[GraphBuff],
        parent_x: f32,
        parent_y: f32,
        draw_x: f32,
        draw_y: f32,
        draw_alpha: f32,
        v3d_x: i32,
        v3d_y: i32,
        v3d_z: i32,
    ) -> Result<(), SoftRenderError> {
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
        let Some(graph) = graphs.get(graph_id as usize) else {
            return Ok(());
        };
        let Some(image) = graph.get_texture().as_ref() else {
            return Ok(());
        };
        let (tw, th) = image.dimensions();
        if tw == 0 || th == 0 {
            return Ok(());
        }

        let attr = prim.get_attr();
        let use_rect = (attr & 1) != 0;
        let (w, h, u, v) = if use_rect {
            let mut w = prim.get_w() as f32;
            let mut h = prim.get_h() as f32;
            if w <= 0.0 {
                w = graph.get_width() as f32;
            }
            if h <= 0.0 {
                h = graph.get_height() as f32;
            }
            (
                w.min(graph.get_width() as f32),
                h.min(graph.get_height() as f32),
                prim.get_u() as f32,
                prim.get_v() as f32,
            )
        } else {
            (
                graph.get_width() as f32,
                graph.get_height() as f32,
                0.0,
                0.0,
            )
        };

        let (pivot_x, pivot_y) = if (attr & 2) != 0 {
            (prim.get_opx() as f32, prim.get_opy() as f32)
        } else {
            (graph.get_u() as f32, graph.get_v() as f32)
        };
        let model = self.build_draw_model(
            prim,
            parent_x,
            parent_y,
            draw_x,
            draw_y,
            graph.get_offset_x() as f32,
            graph.get_offset_y() as f32,
            pivot_x,
            pivot_y,
            v3d_x,
            v3d_y,
            v3d_z,
        );
        self.draw_textured_quad(
            model,
            w,
            h,
            vec2(u / tw as f32, v / th as f32),
            vec2((u + w) / tw as f32, (v + h) / th as f32),
            vec4(1.0, 1.0, 1.0, draw_alpha),
            TextureRef::Graph {
                graph_id,
                graph,
                image,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_prim(
        &mut self,
        prim: &Prim,
        graphs: &[GraphBuff],
        parent_x: f32,
        parent_y: f32,
        draw_x: f32,
        draw_y: f32,
        draw_alpha: f32,
        v3d_x: i32,
        v3d_y: i32,
        v3d_z: i32,
    ) -> Result<(), SoftRenderError> {
        let slot = prim.get_text_index();
        if !(0..=31).contains(&slot) {
            return Ok(());
        }
        let graph_id = 4064u16 + slot as u16;
        let Some(graph) = graphs.get(graph_id as usize) else {
            return Ok(());
        };
        let Some(image) = graph.get_texture().as_ref() else {
            return Ok(());
        };
        let (tw, th) = image.dimensions();
        if tw == 0 || th == 0 {
            return Ok(());
        }

        let attr = prim.get_attr();
        let use_rect = (attr & 1) != 0;
        let display_w = graph.get_display_width() as f32;
        let display_h = graph.get_display_height() as f32;
        let tex_scale_x = if display_w > 0.0 {
            tw as f32 / display_w
        } else {
            1.0
        };
        let tex_scale_y = if display_h > 0.0 {
            th as f32 / display_h
        } else {
            1.0
        };
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
        let (pivot_x, pivot_y) = if (attr & 2) != 0 {
            (prim.get_opx() as f32, prim.get_opy() as f32)
        } else {
            (graph.get_u() as f32, graph.get_v() as f32)
        };
        let model = self.build_draw_model(
            prim,
            parent_x,
            parent_y,
            draw_x,
            draw_y,
            graph.get_offset_x() as f32,
            graph.get_offset_y() as f32,
            pivot_x,
            pivot_y,
            v3d_x,
            v3d_y,
            v3d_z,
        );
        self.draw_textured_quad(
            model,
            w,
            h,
            vec2(tex_u / tw as f32, tex_v / th as f32),
            vec2((tex_u + tex_w) / tw as f32, (tex_v + tex_h) / th as f32),
            vec4(1.0, 1.0, 1.0, draw_alpha),
            TextureRef::Graph {
                graph_id,
                graph,
                image,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_snow_prim(
        &mut self,
        prim: &Prim,
        graphs: &[GraphBuff],
        snow_motions: &[SnowMotion],
        parent_x: f32,
        parent_y: f32,
        draw_x: f32,
        draw_y: f32,
        draw_alpha: f32,
    ) -> Result<(), SoftRenderError> {
        let snow_id = prim.get_texture_id();
        if snow_id < 0 {
            return Ok(());
        }
        let Some(sm) = snow_motions.get(snow_id as usize) else {
            return Ok(());
        };
        if !sm.enabled
            || sm.flake_count <= 0
            || sm.texture_id < 0
            || sm.flake_w <= 1
            || sm.flake_h <= 1
        {
            return Ok(());
        }

        let a0 = sm.color_r as f32;
        let a1 = sm.color_b_or_extra as f32;
        let a2 = sm.color_g as f32;
        let p0 = sm.period_min as f32;
        let p1 = sm.time_override as f32;
        let p2 = sm.period_max as f32;
        let base_tex = sm.texture_id as i32;
        let vcnt = sm.variant_count.max(1) as u32;
        let tile_w_cfg = (sm.flake_w - 1) as f32;
        let tile_h_cfg = (sm.flake_h - 1) as f32;
        let count = sm.flake_count.max(0).min(1024) as usize;
        for j in 0..count {
            let idx = sm.flake_ptrs[j];
            let flake = &sm.flakes[idx];
            let period = flake.period;
            let alpha_u8 = if period <= p0 || p1 <= p0 {
                a0
            } else if period <= p1 {
                a0 + (period - p0) * (a1 - a0) / (p1 - p0)
            } else if period <= p2 && p2 > p1 {
                a1 + (period - p1) * (a2 - a1) / (p2 - p1)
            } else {
                a2
            };
            let alpha = (alpha_u8.clamp(0.0, 255.0) / 255.0) * draw_alpha;
            let graph_i32 = base_tex + (flake.variant_idx % vcnt) as i32;
            if graph_i32 < 0 {
                continue;
            }
            let graph_id = graph_i32 as u16;
            let Some(graph) = graphs.get(graph_id as usize) else {
                continue;
            };
            let Some(image) = graph.get_texture().as_ref() else {
                continue;
            };
            let (tw, th) = image.dimensions();
            if tw == 0 || th == 0 {
                continue;
            }
            let scale = if flake.period > 0.0 {
                1000.0 / flake.period
            } else {
                1.0
            };
            let tile_w = tile_w_cfg.min(tw as f32 - 1.0).max(0.0);
            let tile_h = tile_h_cfg.min(th as f32 - 1.0).max(0.0);
            let w = tile_w * scale;
            let h = tile_h * scale;
            let model = Mat4::from_translation(vec3(
                parent_x + draw_x + flake.x - w * 0.5,
                parent_y + draw_y + flake.y - h * 0.5,
                0.0,
            ));
            self.draw_textured_quad(
                model,
                w,
                h,
                vec2(0.0, 0.0),
                vec2(tile_w / tw as f32, tile_h / th as f32),
                vec4(1.0, 1.0, 1.0, alpha),
                TextureRef::Graph {
                    graph_id,
                    graph,
                    image,
                },
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_tile_prim(
        &mut self,
        prim: &Prim,
        color_manager: &ColorManager,
        parent_x: f32,
        parent_y: f32,
        draw_x: f32,
        draw_y: f32,
        draw_alpha: f32,
        v3d_x: i32,
        v3d_y: i32,
        v3d_z: i32,
    ) {
        let w = prim.get_w() as f32;
        let h = prim.get_h() as f32;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let c = color_manager.get_entry(prim.get_tile() as u8);
        let color = vec4(
            c.get_r() as f32 / 255.0,
            c.get_g() as f32 / 255.0,
            c.get_b() as f32 / 255.0,
            draw_alpha * (c.get_a() as f32 / 255.0),
        );
        let (pivot_x, pivot_y) = if (prim.get_attr() & 2) != 0 {
            (prim.get_opx() as f32, prim.get_opy() as f32)
        } else {
            (0.0, 0.0)
        };
        let model = self.build_draw_model(
            prim, parent_x, parent_y, draw_x, draw_y, 0.0, 0.0, pivot_x, pivot_y, v3d_x, v3d_y,
            v3d_z,
        );
        let _ = self.draw_textured_quad(
            model,
            w,
            h,
            vec2(0.0, 0.0),
            vec2(1.0, 1.0),
            color,
            TextureRef::White,
        );
    }

    #[allow(clippy::too_many_arguments)]
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
        v3d_x: i32,
        v3d_y: i32,
        v3d_z: i32,
    ) -> Mat4 {
        let theta = -(prim.get_angle() as f32) * std::f32::consts::TAU / 3600.0;
        let attr = prim.get_attr();
        let pos_x = parent_x + draw_x;
        let pos_y = parent_y + draw_y;
        let local_x = off_x - pivot_x;
        let local_y = off_y - pivot_y;

        if (attr & 4) != 0 {
            let (vw, vh) = (self.virtual_size.0 as f32, self.virtual_size.1 as f32);
            let center_x = vw * 0.5;
            let center_y = vh * 0.5;
            let mut fx = prim.get_factor_x() as f32;
            let mut fy = prim.get_factor_y() as f32;
            if fx.abs() < 1e-6 {
                fx = 1.0;
            }
            if fy.abs() < 1e-6 {
                fy = 1.0;
            }
            let mut depth = prim.get_z() as f32 - v3d_z as f32;
            if depth.abs() < 1e-3 {
                depth = 1.0;
            }
            let sx = fx / depth;
            let sy = fy / depth;
            let cam_shift_x = 1000.0 * (v3d_x as f32) / fx;
            let cam_shift_y = 1000.0 * (v3d_y as f32) / fy;
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

    fn draw_textured_quad(
        &mut self,
        model: Mat4,
        dst_w: f32,
        dst_h: f32,
        uv0: Vec2,
        uv1: Vec2,
        color: Vec4,
        texture: TextureRef<'_>,
    ) -> Result<(), SoftRenderError> {
        let p0 = model.transform_point3(vec3(0.0, dst_h, 0.0)).truncate();
        let p1 = model.transform_point3(vec3(0.0, 0.0, 0.0)).truncate();
        let p2 = model.transform_point3(vec3(dst_w, dst_h, 0.0)).truncate();
        let p3 = model.transform_point3(vec3(dst_w, 0.0, 0.0)).truncate();
        let v0 = Vertex {
            pos: p0,
            color,
            uv: vec2(uv0.x, uv1.y),
        };
        let v1 = Vertex {
            pos: p1,
            color,
            uv: vec2(uv0.x, uv0.y),
        };
        let v2 = Vertex {
            pos: p2,
            color,
            uv: vec2(uv1.x, uv1.y),
        };
        let v3 = Vertex {
            pos: p3,
            color,
            uv: vec2(uv1.x, uv0.y),
        };
        self.raster_triangle(v0, v1, v2, texture);
        self.raster_triangle(v2, v1, v3, texture);
        self.stats.quad_count += 1;
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Vec4) {
        let model = Mat4::from_translation(vec3(x, y, 0.0));
        let _ = self.draw_textured_quad(
            model,
            w,
            h,
            vec2(0.0, 0.0),
            vec2(1.0, 1.0),
            color,
            TextureRef::White,
        );
    }

    fn raster_triangle(&mut self, a: Vertex, b: Vertex, c: Vertex, texture: TextureRef<'_>) {
        let area = edge(a.pos, b.pos, c.pos);
        if area.abs() <= f32::EPSILON {
            return;
        }
        let min_x = a.pos.x.min(b.pos.x).min(c.pos.x).floor().max(0.0) as i32;
        let min_y = a.pos.y.min(b.pos.y).min(c.pos.y).floor().max(0.0) as i32;
        let max_x = a
            .pos
            .x
            .max(b.pos.x)
            .max(c.pos.x)
            .ceil()
            .min(self.framebuffer.width() as f32) as i32;
        let max_y = a
            .pos
            .y
            .max(b.pos.y)
            .max(c.pos.y)
            .ceil()
            .min(self.framebuffer.height() as f32) as i32;
        if min_x >= max_x || min_y >= max_y {
            return;
        }

        let inv_area = 1.0 / area;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let p = vec2(x as f32 + 0.5, y as f32 + 0.5);
                let w0 = edge(b.pos, c.pos, p) * inv_area;
                let w1 = edge(c.pos, a.pos, p) * inv_area;
                let w2 = edge(a.pos, b.pos, p) * inv_area;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let uv = a.uv * w0 + b.uv * w1 + c.uv * w2;
                let color = a.color * w0 + b.color * w1 + c.color * w2;
                let src = self.sample_texture(texture, uv) * color;
                self.blend_pixel(x as u32, y as u32, src);
            }
        }
    }

    fn sample_texture(&self, texture: TextureRef<'_>, uv: Vec2) -> Vec4 {
        match texture {
            TextureRef::White => Vec4::ONE,
            TextureRef::Graph {
                graph_id,
                graph,
                image,
            } => {
                let nearest = (4064..=4095).contains(&graph_id)
                    || graph.load_kind == GraphBuffLoadKind::GaijiGlyph;
                if nearest {
                    sample_nearest(image, uv)
                } else {
                    sample_linear(image, uv)
                }
            }
        }
    }

    fn blend_pixel(&mut self, x: u32, y: u32, src: Vec4) {
        let src_a = src.w.clamp(0.0, 1.0);
        if src_a <= 0.0 {
            return;
        }
        let width = self.framebuffer.width();
        let height = self.framebuffer.height();
        if x >= width || y >= height {
            return;
        }
        let format = self.framebuffer.format();
        let stride = self.framebuffer.stride();
        let off = y as usize * stride + x as usize * format.bytes_per_pixel();
        let pixels = self.framebuffer.pixels_mut();
        let (dr, dg, db, da) = match format {
            PixelFormat::Rgba8 => (
                pixels[off] as f32 / 255.0,
                pixels[off + 1] as f32 / 255.0,
                pixels[off + 2] as f32 / 255.0,
                pixels[off + 3] as f32 / 255.0,
            ),
            PixelFormat::Bgra8 => (
                pixels[off + 2] as f32 / 255.0,
                pixels[off + 1] as f32 / 255.0,
                pixels[off] as f32 / 255.0,
                pixels[off + 3] as f32 / 255.0,
            ),
        };
        let inv = 1.0 - src_a;
        let out = vec4(
            src.x.clamp(0.0, 1.0) * src_a + dr * inv,
            src.y.clamp(0.0, 1.0) * src_a + dg * inv,
            src.z.clamp(0.0, 1.0) * src_a + db * inv,
            src_a + da * inv,
        );
        let r = (out.x.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (out.y.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (out.z.clamp(0.0, 1.0) * 255.0).round() as u8;
        let a = (out.w.clamp(0.0, 1.0) * 255.0).round() as u8;
        match format {
            PixelFormat::Rgba8 => pixels[off..off + 4].copy_from_slice(&[r, g, b, a]),
            PixelFormat::Bgra8 => pixels[off..off + 4].copy_from_slice(&[b, g, r, a]),
        }
    }
}

fn edge(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    (p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x)
}

fn sample_nearest(image: &DynamicImage, uv: Vec2) -> Vec4 {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return Vec4::ZERO;
    }
    let u = uv.x.clamp(0.0, 1.0);
    let v = uv.y.clamp(0.0, 1.0);
    let x = (u * (w.saturating_sub(1)) as f32).round() as u32;
    let y = (v * (h.saturating_sub(1)) as f32).round() as u32;
    rgba_to_vec4(image.get_pixel(x.min(w - 1), y.min(h - 1)).0)
}

fn sample_linear(image: &DynamicImage, uv: Vec2) -> Vec4 {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return Vec4::ZERO;
    }
    let u = uv.x.clamp(0.0, 1.0) * (w.saturating_sub(1)) as f32;
    let v = uv.y.clamp(0.0, 1.0) * (h.saturating_sub(1)) as f32;
    let x0 = u.floor() as u32;
    let y0 = v.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let tx = u - x0 as f32;
    let ty = v - y0 as f32;
    let c00 = rgba_to_vec4(image.get_pixel(x0, y0).0);
    let c10 = rgba_to_vec4(image.get_pixel(x1, y0).0);
    let c01 = rgba_to_vec4(image.get_pixel(x0, y1).0);
    let c11 = rgba_to_vec4(image.get_pixel(x1, y1).0);
    c00.lerp(c10, tx).lerp(c01.lerp(c11, tx), ty)
}

fn rgba_to_vec4(px: [u8; 4]) -> Vec4 {
    vec4(
        px[0] as f32 / 255.0,
        px[1] as f32 / 255.0,
        px[2] as f32 / 255.0,
        px[3] as f32 / 255.0,
    )
}
