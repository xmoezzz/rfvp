use std::{collections::HashMap, sync::Arc};

use glam::{mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use image::{DynamicImage, GenericImageView};

use crate::{rfvp_render::{
    GpuCommonResources, GpuTexture, TextureBindGroup, VertexBuffer, pipelines::sprite::SpritePipeline, vertices::{PosColTexVertex, VertexSource}
}, subsystem::resources::{color_manager::ColorManager, motion_manager::{MotionManager, snow::SnowMotion}}};

use crate::subsystem::resources::{graph_buff::GraphBuff, prim::{Prim, PrimManager, PrimType}};

#[derive(Clone, Copy, Debug)]
enum DrawTextureKey {
    Graph(u16),
    White,
}

#[derive(Clone, Debug)]
struct DrawItem {
    tex: DrawTextureKey,
    vertex_range: std::ops::Range<u32>,
}

#[derive(Debug)]
struct GraphGpuEntry {
    generation: u64,
    texture: GpuTexture,
}

/// GPU primitive renderer for Sprt/Tile/Group traversal.
///
/// Design goals:
/// - Preserve scene graph draw order.
/// - Upload GraphBuff images to GPU lazily and refresh on generation bumps.
/// - Keep rendering code independent from scripting/syscall layers.
pub struct GpuPrimRenderer {
    virtual_size: (u32, u32),
    vb: VertexBuffer<PosColTexVertex>,
    vb_capacity: u32,
    vertices: Vec<PosColTexVertex>,
    draws: Vec<DrawItem>,
    graph_cache: HashMap<u16, GraphGpuEntry>,
    white: GpuTexture,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PrimRenderStats {
    pub quad_count: usize,
    pub vertex_count: usize,
    pub draw_calls: usize,
    pub cached_graphs: usize,
}

impl GpuPrimRenderer {
    pub fn new(resources: Arc<GpuCommonResources>, virtual_size: (u32, u32)) -> Self {
        let vb_capacity = 1024 * 6; // initial: 1024 quads
        let vb = VertexBuffer::new_updatable(resources.as_ref(), vb_capacity, Some("GpuPrimRenderer.vertex_buffer"));

        let white = {
            let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
            GpuTexture::new(resources.as_ref(), &DynamicImage::ImageRgba8(img), Some("white_1x1"))
        };

        Self {
            virtual_size,
            vb,
            vb_capacity,
            vertices: Vec::with_capacity(vb_capacity as usize),
            draws: Vec::new(),
            graph_cache: HashMap::new(),
            white,
        }
    }

    pub fn set_virtual_size(&mut self, virtual_size: (u32, u32)) {
        self.virtual_size = virtual_size;
    }

    pub fn stats(&self) -> PrimRenderStats {
        // Each quad is two triangles => 6 vertices.
        let vertex_count = self.vertices.len();
        let quad_count = vertex_count / 6;
        PrimRenderStats {
            quad_count,
            vertex_count,
            draw_calls: self.draws.len(),
            cached_graphs: self.graph_cache.len(),
        }
    }

    /// Rebuild renderer draw lists from the current motion manager state.
    /// This matches the callsite in `app.rs`.
    pub fn rebuild(&mut self, resources: &GpuCommonResources, motion: &MotionManager) {
        self.build(resources, motion);
    }

    fn ensure_vb_capacity(&mut self, resources: &GpuCommonResources, needed_vertices: u32) {
        if needed_vertices <= self.vb_capacity {
            return;
        }

        let mut new_cap = self.vb_capacity.max(1);
        while new_cap < needed_vertices {
            new_cap = new_cap.saturating_mul(2);
        }

        self.vb = VertexBuffer::new_updatable(resources, new_cap, Some("GpuPrimRenderer.vertex_buffer"));
        self.vb_capacity = new_cap;
        self.vertices.reserve((new_cap - self.vertices.len() as u32) as usize);
    }

    fn virtual_projection(&self) -> Mat4 {
        // Virtual space: origin at top-left, x right, y down.
        let (w, h) = (self.virtual_size.0 as f32, self.virtual_size.1 as f32);
        mat4(
            vec4(2.0 / w, 0.0, 0.0, 0.0),
            vec4(0.0, -2.0 / h, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(-1.0, 1.0, 0.0, 1.0),
        )
    }

    fn build_local_transform(&self, prim: &Prim) -> Mat4 {        let x = prim.get_x() as f32;
        let y = prim.get_y() as f32;
        let opx = prim.get_opx() as f32;
        let opy = prim.get_opy() as f32;
        let sx = prim.get_factor_x() as f32 / 1000.0;
        let sy = prim.get_factor_y() as f32 / 1000.0;

        // Screen space uses y-down; use negative angle to keep expected clockwise positive.
        let theta = -(prim.get_angle() as f32) * std::f32::consts::PI / 180.0;

        Mat4::from_translation(vec3(x, y, 0.0))
            * Mat4::from_translation(vec3(opx, opy, 0.0))
            * Mat4::from_rotation_z(theta)
            * Mat4::from_scale(vec3(sx, sy, 1.0))
            * Mat4::from_translation(vec3(-opx, -opy, 0.0))
    }

    fn upload_graph_if_needed(
        &mut self,
        resources: &GpuCommonResources,
        graph_id: u16,
        graph: &GraphBuff,
    ) {
        let gen = graph.get_generation();

        let needs_upload = match self.graph_cache.get(&graph_id) {
            Some(e) => e.generation != gen,
            None => true,
        };

        if !needs_upload {
            return;
        }

        let Some(img) = graph.get_texture().as_ref() else {
            return;
        };

        let tex = GpuTexture::new(resources, img, Some(&format!("graph_{}", graph_id)));
        self.graph_cache.insert(
            graph_id,
            GraphGpuEntry {
                generation: gen,
                texture: tex,
            },
        );
    }

    fn texture_bind_group(&self, key: DrawTextureKey) -> &TextureBindGroup {
        match key {
            DrawTextureKey::White => self.white.bind_group(),
            DrawTextureKey::Graph(id) => self
                .graph_cache
                .get(&id)
                .expect("graph texture must be uploaded before use")
                .texture
                .bind_group(),
        }
    }

    fn emit_sprite_vertices(
        &mut self,
        model: Mat4,
        dst_w: f32,
        dst_h: f32,
        uv0: Vec2,
        uv1: Vec2,
        color: Vec4,
        tex: DrawTextureKey,
    ) {
        let base = self.vertices.len() as u32;

        // Two triangles (0,1,2) (2,1,3)
        let p0 = model.transform_point3(vec3(0.0, dst_h, 0.0));
        let p1 = model.transform_point3(vec3(0.0, 0.0, 0.0));
        let p2 = model.transform_point3(vec3(dst_w, dst_h, 0.0));
        let p3 = model.transform_point3(vec3(dst_w, 0.0, 0.0));

        let v0 = PosColTexVertex {
            position: p0,
            color,
            texture_coordinate: vec2(uv0.x, uv1.y),
        };
        let v1 = PosColTexVertex {
            position: p1,
            color,
            texture_coordinate: vec2(uv0.x, uv0.y),
        };
        let v2 = PosColTexVertex {
            position: p2,
            color,
            texture_coordinate: vec2(uv1.x, uv1.y),
        };
        let v3 = PosColTexVertex {
            position: p3,
            color,
            texture_coordinate: vec2(uv1.x, uv0.y),
        };

        self.vertices.extend_from_slice(&[v0, v1, v2, v2, v1, v3]);
        self.draws.push(DrawItem {
            tex,
            vertex_range: base..base + 6,
        });
    }


    fn collect_tree(
        &mut self,
        resources: &GpuCommonResources,
        prim_manager: &PrimManager,
        color_manager: &ColorManager,
        graphs: &[GraphBuff],
        snow_motions: &[SnowMotion],
        prim_id: i16,
        acc_x: f32,
        acc_y: f32,
        visit: &mut [u8],
        depth: usize,
    ) {
        if prim_id < 0 {
            return;
        }

        let prim_idx = prim_id as usize;
        if prim_idx >= visit.len() {
            log::error!("collect_tree: invalid prim_id {prim_id}");
            return;
        }
        if depth > 4096 {
            log::error!("collect_tree: depth overflow at prim_id {prim_id}");
            return;
        }
        if visit[prim_idx] == 1 {
            log::error!("collect_tree: cycle detected at prim_id {prim_id}");
            return;
        }
        if visit[prim_idx] == 2 {
            // Duplicate reference (should not happen in a tree). Skip.
            return;
        }
        visit[prim_idx] = 1;

        // -----------------------------
        // Base prim (container semantics)
        // -----------------------------
        // base prim is used for:
        //  - container translation (x/y)
        //  - alpha (after sprt impersonation override per your comment)
        //  - child traversal (first_child)
        let base_prim = prim_manager.get_prim_immutable(prim_id);
        if !base_prim.get_draw_flag() {
            visit[prim_idx] = 2;
            return;
        }

        let base_x = base_prim.get_x() as f32;
        let base_y = base_prim.get_y() as f32;
        let base_a = base_prim.get_alpha() as f32 / 255.0;
        let first_child = base_prim.get_first_child_idx();

        // Sprt impersonation: follow m_Sprt chain; resolve final draw prim id.
        // IDA semantics you described: draw prim can be impersonated via sprt chain.
        let mut draw_id: i16 = prim_id;
        let mut sprt: i16 = base_prim.get_sprt();

        // base_prim no longer needed beyond captured locals; it will drop naturally later,
        // but we don't rely on it anymore to avoid move/borrow issues.

        while sprt != -1 {
            if sprt < 0 || sprt >= 4096 {
                log::error!("collect_tree: invalid sprt id {sprt} under prim_id {prim_id}");
                visit[prim_idx] = 2;
                return;
            }

            let sref = prim_manager.get_prim_immutable(sprt);
            if !sref.get_draw_flag() {
                visit[prim_idx] = 2;
                return;
            }

            draw_id = sprt;
            sprt = sref.get_sprt();
            // sref dropped here
        }

        // Borrow final draw prim for renderable properties (w/h/u/v/transform/type/etc.)
        let draw_prim = prim_manager.get_prim_immutable(draw_id);

        // Accumulate container translation only (IDA: x + base->m_X, y + base->m_Y).
        let x = acc_x + base_x;
        let y = acc_y + base_y;

        println!(
            "[prim] depth={} prim_id={} draw_id={} base(x,y)=({:.2},{:.2}) acc(x,y)=({:.2},{:.2}) => x,y=({:.2},{:.2}) alpha={:.3} type={:?} sprt_first={}",
            depth,
            prim_id,
            draw_id,
            base_x,
            base_y,
            acc_x,
            acc_y,
            x,
            y,
            base_a,
            draw_prim.get_type(),
            base_prim.get_sprt(),
        );

        // Local transform for renderable prims: translate to (x,y), then pivot/rotate/scale around draw_prim.op.
        let model = {
            let opx = draw_prim.get_opx() as f32;
            let opy = draw_prim.get_opy() as f32;
            let sx = draw_prim.get_factor_x() as f32 / 1000.0;
            let sy = draw_prim.get_factor_y() as f32 / 1000.0;
            let theta = -(draw_prim.get_angle() as f32) * std::f32::consts::PI / 180.0;

            Mat4::from_translation(vec3(x, y, 0.0))
                * Mat4::from_translation(vec3(opx, opy, 0.0))
                * Mat4::from_rotation_z(theta)
                * Mat4::from_scale(vec3(sx, sy, 1.0))
                * Mat4::from_translation(vec3(-opx, -opy, 0.0))
        };

        match draw_prim.get_type() {
            PrimType::PrimTypeGroup => {
                // No draw; traverse children.
            }

            PrimType::PrimTypeSprt => {
                let tex_id = draw_prim.get_texture_id();
                let graph_id = if tex_id >= 0 {
                    Some(tex_id as u16)
                } else if tex_id == -2 {
                    Some(crate::subsystem::resources::videoplayer::MOVIE_GRAPH_ID)
                } else {
                    None
                };

                if let Some(tex_id) = graph_id {
                    if let Some(g) = graphs.get(tex_id as usize) {
                        self.upload_graph_if_needed(resources, tex_id, g);
                        if self.graph_cache.contains_key(&tex_id) {
                            let (tw, th) = match g.get_texture().as_ref() {
                                Some(img) => img.dimensions(),
                                None => (0, 0),
                            };
                            if tw > 0 && th > 0 {
                                let mut w = draw_prim.get_w() as f32;
                                let mut h = draw_prim.get_h() as f32;
                                if w <= 0.0 {
                                    w = g.get_width() as f32;
                                }
                                if h <= 0.0 {
                                    h = g.get_height() as f32;
                                }

                                let attr = draw_prim.get_attr();
                                let use_rect = (attr & 1) != 0;

                                let (mut w, mut h, mut u, mut v) = if use_rect {
                                    let mut w = draw_prim.get_w() as f32;
                                    let mut h = draw_prim.get_h() as f32;
                                    if w <= 0.0 { w = g.get_width() as f32; }
                                    if h <= 0.0 { h = g.get_height() as f32; }

                                    // IDA: clamp to graph_width/height
                                    w = w.min(g.get_width() as f32);
                                    h = h.min(g.get_height() as f32);

                                    (w, h, draw_prim.get_u() as f32, draw_prim.get_v() as f32)
                                } else {
                                    (g.get_width() as f32, g.get_height() as f32, 0.0, 0.0)
                                };

                                let u = draw_prim.get_u() as f32;
                                let v = draw_prim.get_v() as f32;
                                let uv0 = vec2(u / tw as f32, v / th as f32);
                                let uv1 = vec2((u + w) / tw as f32, (v + h) / th as f32);

                                // Alpha comes from base prim (original container prim).
                                let color = vec4(1.0, 1.0, 1.0, base_a);

                                let model = {
                                    let opx = draw_prim.get_opx() as f32;
                                    let opy = draw_prim.get_opy() as f32;
                                    let sx = draw_prim.get_factor_x() as f32 / 1000.0;
                                    let sy = draw_prim.get_factor_y() as f32 / 1000.0;
                                    let theta = -(draw_prim.get_angle() as f32) * std::f32::consts::PI / 180.0;

                                    let off_x = g.get_offset_x() as f32;
                                    let off_y = g.get_offset_y() as f32;

                                    Mat4::from_translation(vec3(x + off_x, y + off_y, 0.0))
                                        * Mat4::from_translation(vec3(opx, opy, 0.0))
                                        * Mat4::from_rotation_z(theta)
                                        * Mat4::from_scale(vec3(sx, sy, 1.0))
                                        * Mat4::from_translation(vec3(-opx, -opy, 0.0))
                                };

                                self.emit_sprite_vertices(
                                    model,
                                    w,
                                    h,
                                    uv0,
                                    uv1,
                                    color,
                                    DrawTextureKey::Graph(tex_id),
                                );
                            }
                        }
                    }
                }
            }

            PrimType::PrimTypeText => {
                // Text primitives own a dedicated texture slot (0..31) mapped to GraphBuff[4064 + slot].
                let slot = draw_prim.get_text_index();
                if (0..=31).contains(&slot) {
                    let graph_id = 4064u16 + slot as u16;
                    if let Some(g) = graphs.get(graph_id as usize) {
                        self.upload_graph_if_needed(resources, graph_id, g);
                        if self.graph_cache.contains_key(&graph_id) {
                            let (tw, th) = match g.get_texture().as_ref() {
                                Some(img) => img.dimensions(),
                                None => (0, 0),
                            };
                            if tw > 0 && th > 0 {
                                let mut w = draw_prim.get_w() as f32;
                                let mut h = draw_prim.get_h() as f32;
                                if w <= 0.0 {
                                    w = g.get_width() as f32;
                                }
                                if h <= 0.0 {
                                    h = g.get_height() as f32;
                                }

                                let attr = draw_prim.get_attr();
                                let use_rect = (attr & 1) != 0;

                                let (mut w, mut h, mut u, mut v) = if use_rect {
                                    let mut w = draw_prim.get_w() as f32;
                                    let mut h = draw_prim.get_h() as f32;
                                    if w <= 0.0 { w = g.get_width() as f32; }
                                    if h <= 0.0 { h = g.get_height() as f32; }

                                    // IDA: clamp to graph_width/height
                                    w = w.min(g.get_width() as f32);
                                    h = h.min(g.get_height() as f32);

                                    (w, h, draw_prim.get_u() as f32, draw_prim.get_v() as f32)
                                } else {
                                    (g.get_width() as f32, g.get_height() as f32, 0.0, 0.0)
                                };

                                let u = draw_prim.get_u() as f32;
                                let v = draw_prim.get_v() as f32;
                                let uv0 = vec2(u / tw as f32, v / th as f32);
                                let uv1 = vec2((u + w) / tw as f32, (v + h) / th as f32);

                                let color = vec4(1.0, 1.0, 1.0, base_a);

                                let model = {
                                    let opx = draw_prim.get_opx() as f32;
                                    let opy = draw_prim.get_opy() as f32;
                                    let sx = draw_prim.get_factor_x() as f32 / 1000.0;
                                    let sy = draw_prim.get_factor_y() as f32 / 1000.0;
                                    let theta = -(draw_prim.get_angle() as f32) * std::f32::consts::PI / 180.0;

                                    let off_x = g.get_offset_x() as f32;
                                    let off_y = g.get_offset_y() as f32;

                                    Mat4::from_translation(vec3(x + off_x, y + off_y, 0.0))
                                        * Mat4::from_translation(vec3(opx, opy, 0.0))
                                        * Mat4::from_rotation_z(theta)
                                        * Mat4::from_scale(vec3(sx, sy, 1.0))
                                        * Mat4::from_translation(vec3(-opx, -opy, 0.0))
                                };

                                self.emit_sprite_vertices(
                                    model,
                                    w,
                                    h,
                                    uv0,
                                    uv1,
                                    color,
                                    DrawTextureKey::Graph(graph_id),
                                );
                            }
                        }
                    }
                }
            }

            PrimType::PrimTypeSnow => {
                // Snow flakes are positioned relative to (x,y).
                let snow_id = draw_prim.get_texture_id();
                if snow_id >= 0 {
                    if let Some(sm) = snow_motions.get(snow_id as usize) {
                        if sm.enabled
                            && sm.flake_count > 0
                            && sm.texture_id >= 0
                            && sm.flake_w > 1
                            && sm.flake_h > 1
                        {
                            let color = vec4(
                                sm.color_r as f32 / 255.0,
                                sm.color_g as f32 / 255.0,
                                sm.color_b_or_extra as f32 / 255.0,
                                base_a,
                            );

                            let base_tex = sm.texture_id as i32;
                            let vcnt = sm.variant_count.max(1) as u32;

                            let tile_w_cfg = (sm.flake_w - 1) as f32;
                            let tile_h_cfg = (sm.flake_h - 1) as f32;

                            let count = sm.flake_count.max(0).min(1024) as usize;
                            for j in 0..count {
                                let idx = sm.flake_ptrs[j];
                                let flake = &sm.flakes[idx];

                                let vi = (flake.variant_idx % vcnt) as i32;
                                let graph_i32 = base_tex + vi;
                                if graph_i32 < 0 {
                                    continue;
                                }
                                let graph_id = graph_i32 as u16;

                                if let Some(g) = graphs.get(graph_id as usize) {
                                    self.upload_graph_if_needed(resources, graph_id, g);
                                    if !self.graph_cache.contains_key(&graph_id) {
                                        continue;
                                    }

                                    let (tw, th) = match g.get_texture().as_ref() {
                                        Some(img) => img.dimensions(),
                                        None => (0, 0),
                                    };
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

                                    let u0 = 0.0;
                                    let v0 = 0.0;
                                    let u1 = tile_w / tw as f32;
                                    let v1 = tile_h / th as f32;

                                    let flake_model = Mat4::from_translation(vec3(x, y, 0.0))
                                        * Mat4::from_translation(vec3(
                                            flake.x - w * 0.5,
                                            flake.y - h * 0.5,
                                            0.0,
                                        ));

                                    self.emit_sprite_vertices(
                                        flake_model,
                                        w,
                                        h,
                                        vec2(u0, v0),
                                        vec2(u1, v1),
                                        color,
                                        DrawTextureKey::Graph(graph_id),
                                    );
                                }
                            }
                        }
                    }
                }
            }

            PrimType::PrimTypeTile => {
                let w = draw_prim.get_w() as f32;
                let h = draw_prim.get_h() as f32;
                let color_id = draw_prim.get_tile();
                let c = color_manager.get_entry(color_id as u8);
                if w > 0.0 && h > 0.0 {
                    let color = vec4(
                        c.get_r() as f32 / 255.0,
                        c.get_g() as f32 / 255.0,
                        c.get_b() as f32 / 255.0,
                        base_a,
                    );

                    self.emit_sprite_vertices(
                        model,
                        w,
                        h,
                        vec2(0.0, 0.0),
                        vec2(1.0, 1.0),
                        color,
                        DrawTextureKey::White,
                    );
                }
            }

            PrimType::PrimTypeNone => {
                // No draw; traverse children.
            }

            _ => {}
        }

        // -----------------------------
        // Traverse children (base prim semantics)
        // Note: container uses (acc + base.X/Y) only; scale/rotation do not affect children.
        // -----------------------------
        let mut children: Vec<i16> = Vec::new();
        let mut child = first_child;
        let mut steps: usize = 0;

        while child != -1 {
            if steps >= 4096 {
                log::error!(
                    "collect_tree: child sibling chain too long (possible cycle) at prim_id {prim_id}"
                );
                break;
            }
            steps += 1;

            if child < 0 || child >= 4096 {
                log::error!("collect_tree: invalid child id {child} under prim_id {prim_id}");
                break;
            }

            children.push(child);

            let p = prim_manager.get_prim_immutable(child);
            child = p.get_next_sibling_idx();
        }

        if children.len() > 1 {
            children.sort_by_key(|&cid| {
                let p = prim_manager.get_prim_immutable(cid);
                (p.get_z(), cid)
            });
        }

        for cid in children {
            self.collect_tree(
                resources,
                prim_manager,
                color_manager,
                graphs,
                snow_motions,
                cid,
                x,
                y,
                visit,
                depth + 1,
            );
        }

        visit[prim_idx] = 2;
    }

    /// Build geometry and draw items from the current primitive tree.
    pub fn build(&mut self, resources: &GpuCommonResources, motion: &MotionManager) {
        self.vertices.clear();
        self.draws.clear();

        let prim_manager = motion.prim_manager();
        let graphs = motion.graphs();
        let snow_motions = motion.snow_motions();

        // 1) draw root tree (slot 0) always
        {
            let mut visit = vec![0u8; 4096];
            self.collect_tree(
                resources,
                prim_manager,
                &motion.color_manager,
                graphs,
                snow_motions,
                0, // root
                0.0,
                0.0,
                &mut visit,
                0,
            );
        }

        // 2) draw optional overlay root if non-zero
        let root = prim_manager.get_custom_root_prim_id() as i16;
        if root != 0 {
            let mut visit = vec![0u8; 4096]; // do NOT reuse visit; allow redraw
            self.collect_tree(
                resources,
                prim_manager,
                &motion.color_manager,
                graphs,
                snow_motions,
                root,
                0.0,
                0.0,
                &mut visit,
                0,
            );
        }

        self.ensure_vb_capacity(resources, self.vertices.len() as u32);
        self.vb.write(&resources.queue, &self.vertices);
    }


    fn draw_with_proj<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        sprite_pipeline: &'a SpritePipeline,
        proj: Mat4,
    ) {
        for item in &self.draws {
            let src = self.vb.vertex_source_slice(item.vertex_range.clone());
            sprite_pipeline.draw(render_pass, src, self.texture_bind_group(item.tex), proj);
        }
    }

    pub fn draw<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        sprite_pipeline: &'a SpritePipeline,
    ) {
        let proj = self.virtual_projection();
        self.draw_with_proj(render_pass, sprite_pipeline, proj);
    }

    /// Draw using an externally provided projection matrix (used by app.rs).
    pub fn draw_virtual<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        sprite_pipeline: &'a SpritePipeline,
        projection_matrix: Mat4,
    ) {
        self.draw_with_proj(render_pass, sprite_pipeline, projection_matrix);
    }
}
