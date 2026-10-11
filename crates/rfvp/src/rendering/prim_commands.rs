#[cfg(feature = "no_std")]
use alloc::{vec, vec::Vec};

#[cfg(feature = "no_std")]
use core_maths::CoreFloat;
use glam::{vec2, vec3, vec4, Mat4, Vec2, Vec4};

use crate::host_api::{
    CommandBlendMode, DrawImageCmd, HitProxy, HitProxyTable, PortableTextureDesc, PrimId, RectI16,
    RectU16, RenderBackend, RenderCommand, RenderFrame, RfvpError, RfvpResult, Rgba8,
    TextureBackend, TextureFormat, TextureHandle, Vertex2D,
};
use crate::subsystem::resources::{
    color_manager::ColorManager,
    graph_buff::GraphBuff,
    motion_manager::{snow::SnowMotion, DissolveType, MotionManager},
    prim::{Prim, PrimManager, PrimType},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HostGraphCacheEntry {
    pub graph_id: u16,
    pub generation: u64,
}

#[derive(Debug, Default)]
pub(crate) struct HostPrimRenderCache {
    graphs: Vec<HostGraphCacheEntry>,
    white_ready: bool,
}

impl HostPrimRenderCache {
    pub fn new() -> Self {
        Self {
            graphs: Vec::new(),
            white_ready: false,
        }
    }

    /// Whether the host holds `graph_id` at `generation` (so its CPU copy may be evicted).
    pub fn is_graph_uploaded(&self, graph_id: u16, generation: u64) -> bool {
        self.graph_generation(graph_id) == Some(generation)
    }

    /// Upload one graph now (see `MotionManager::upload_evicted_one_by_one`).
    #[cfg(feature = "psx")]
    pub fn upload_graph<B>(&mut self, backend: &mut B, graph_id: u16, graph: &GraphBuff) -> bool
    where
        B: TextureBackend<Error = RfvpError>,
    {
        match self.upload_graph_if_needed(backend, graph_id, graph) {
            Ok(done) => done,
            Err(err) => {
                log::error!("graph {graph_id}: upload failed: {err:?}");
                false
            }
        }
    }

    /// Free the host's copy of a graph the game unloaded.
    pub fn remove_graph<B>(&mut self, backend: &mut B, graph_id: u16)
    where
        B: TextureBackend<Error = RfvpError>,
    {
        if let Some(index) = self.graphs.iter().position(|entry| entry.graph_id == graph_id) {
            self.graphs.swap_remove(index);
            backend.destroy_texture(host_texture_id(graph_id));
        }
    }

    fn graph_generation(&self, graph_id: u16) -> Option<u64> {
        self.graphs
            .iter()
            .find(|entry| entry.graph_id == graph_id)
            .map(|entry| entry.generation)
    }

    fn set_graph_generation(&mut self, graph_id: u16, generation: u64) {
        if let Some(entry) = self
            .graphs
            .iter_mut()
            .find(|entry| entry.graph_id == graph_id)
        {
            entry.generation = generation;
            return;
        }
        self.graphs.push(HostGraphCacheEntry {
            graph_id,
            generation,
        });
    }

    fn ensure_white<B>(&mut self, backend: &mut B) -> RfvpResult<()>
    where
        B: TextureBackend<Error = RfvpError>,
    {
        if self.white_ready {
            return Ok(());
        }
        backend.create_texture(
            WHITE_TEXTURE,
            PortableTextureDesc {
                width: 1,
                height: 1,
                format: TextureFormat::Rgba8,
            },
            &[255, 255, 255, 255],
        )?;
        self.white_ready = true;
        Ok(())
    }

    fn upload_graph_if_needed<B>(
        &mut self,
        backend: &mut B,
        graph_id: u16,
        graph: &GraphBuff,
    ) -> RfvpResult<bool>
    where
        B: TextureBackend<Error = RfvpError>,
    {
        let generation = graph.get_generation();
        if self.graph_generation(graph_id) == Some(generation) {
            // Uploaded already; its CPU pixels may have been evicted since.
            return Ok(true);
        }
        let Some(img) = graph.get_texture().as_ref() else {
            return Ok(false);
        };

        let (width, height) = img.dimensions();
        let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
            log::error!("graph {graph_id} is {width}x{height}: too large for a texture");
            return Err(RfvpError::CapacityExceeded);
        };
        let (format, pixels) = match img {
            crate::DynamicImage::ImageRgba8(img) => (TextureFormat::Rgba8, img.as_raw().as_slice()),
            crate::DynamicImage::ImageLumaA8(img) => {
                (TextureFormat::LumaA8, img.as_raw().as_slice())
            }
            #[cfg(feature = "psx")]
            crate::DynamicImage::ImageClut(img) => {
                let format = if img.bits == 4 { TextureFormat::Indexed4 } else { TextureFormat::Indexed8 };
                (format, img.texture_bytes())
            }
        };
        backend.create_texture(
            host_texture_id(graph_id),
            PortableTextureDesc {
                width,
                height,
                format,
            },
            pixels,
        )?;
        self.set_graph_generation(graph_id, generation);
        Ok(true)
    }
}

#[derive(Clone, Copy, Debug)]
enum DrawTextureKey {
    Graph(u16),
    White,
}

const WHITE_TEXTURE: TextureHandle = TextureHandle(u32::MAX);

fn host_texture_id(graph_id: u16) -> TextureHandle {
    TextureHandle(graph_id as u32)
}

fn build_draw_model(
    virtual_size: (u32, u32),
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
    // Most console UI primitives only translate. Avoid trigonometry and four
    // general matrix products on CPUs without hardware floating point.
    if prim.get_attr() & 4 == 0 && prim.get_angle() == 0 {
        let pos_x = parent_x + draw_x;
        let pos_y = parent_y + draw_y;
        if prim.get_factor_x() == 1000 && prim.get_factor_y() == 1000 {
            return Mat4::from_translation(vec3(pos_x + off_x, pos_y + off_y, 0.0));
        }
        let sx = prim.get_factor_x() as f32 / 1000.0;
        let sy = prim.get_factor_y() as f32 / 1000.0;
        return Mat4::from_cols_array(&[
            sx, 0.0, 0.0, 0.0,
            0.0, sy, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            pos_x + pivot_x + sx * (off_x - pivot_x),
            pos_y + pivot_y + sy * (off_y - pivot_y), 0.0, 1.0,
        ]);
    }
    let theta = -(prim.get_angle() as f32) * core::f32::consts::TAU / 3600.0;
    let attr = prim.get_attr();
    let pos_x = parent_x + draw_x;
    let pos_y = parent_y + draw_y;
    let local_x = off_x - pivot_x;
    let local_y = off_y - pivot_y;

    if (attr & 4) != 0 {
        let (vw, vh) = (virtual_size.0 as f32, virtual_size.1 as f32);
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

/// `PrimSetBlend(id, 1)` selects additive blending (SRCALPHA/ONE) in the original engine.
fn prim_blend(prim: &Prim) -> CommandBlendMode {
    if prim.get_blend() {
        CommandBlendMode::Add
    } else {
        CommandBlendMode::Normal
    }
}

fn sprite_corners(model: Mat4, dst_w: f32, dst_h: f32) -> [glam::Vec3; 4] {
    // Share the transformed origin and edges, including for rotated quads.
    let p1 = model.w_axis.truncate();
    let edge_x = model.x_axis.truncate() * dst_w;
    let edge_y = model.y_axis.truncate() * dst_h;
    let p0 = p1 + edge_y;
    let p3 = p1 + edge_x;
    [p0, p1, p3 + edge_y, p3]
}

fn emit_sprite(
    commands: &mut Vec<RenderCommand>,
    hit_proxies: &mut HitProxyTable,
    order: &mut u32,
    prim_id: i16,
    model: Mat4,
    dst_w: f32,
    dst_h: f32,
    uv0: Vec2,
    uv1: Vec2,
    color: Vec4,
    texture: DrawTextureKey,
    blend: CommandBlendMode,
) -> RfvpResult<()> {
    let [p0, p1, p2, p3] = sprite_corners(model, dst_w, dst_h);
    let texture = match texture {
        DrawTextureKey::Graph(graph_id) => host_texture_id(graph_id),
        DrawTextureKey::White => WHITE_TEXTURE,
    };
    let vertices = [
        vertex(p0, vec2(uv0.x, uv1.y), color),
        vertex(p1, vec2(uv0.x, uv0.y), color),
        vertex(p2, vec2(uv1.x, uv1.y), color),
        vertex(p3, vec2(uv1.x, uv0.y), color),
    ];
    let rect = vertices_aabb(&vertices);

    commands.push(RenderCommand::DrawImage(DrawImageCmd {
        texture,
        src: RectU16 {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
        dst: rect,
        color: rgba8_from_vec4(color),
        blend,
        effect_id: 0,
        clip: None,
        vertices,
    }));
    if rect.w > 0 && rect.h > 0 && prim_id >= 0 {
        hit_proxies.push(HitProxy {
            prim_id: PrimId(prim_id as u32),
            rect,
            enabled: true,
            visible: color.w > 0.0,
            order: *order,
        });
    }
    *order = order.wrapping_add(1);
    Ok(())
}

fn vertex(position: glam::Vec3, tex_coord: Vec2, color: Vec4) -> Vertex2D {
    Vertex2D {
        position: [position.x, position.y],
        tex_coord: [tex_coord.x, tex_coord.y],
        color: crate::host_api::ColorRgba {
            r: color.x,
            g: color.y,
            b: color.z,
            a: color.w,
        },
    }
}

fn rgba8_from_vec4(color: Vec4) -> Rgba8 {
    Rgba8 {
        r: (color.x.clamp(0.0, 1.0) * 255.0) as u8,
        g: (color.y.clamp(0.0, 1.0) * 255.0) as u8,
        b: (color.z.clamp(0.0, 1.0) * 255.0) as u8,
        a: (color.w.clamp(0.0, 1.0) * 255.0) as u8,
    }
}

fn vertices_aabb(vertices: &[Vertex2D; 4]) -> RectI16 {
    let mut min_x = vertices[0].position[0];
    let mut max_x = min_x;
    let mut min_y = vertices[0].position[1];
    let mut max_y = min_y;
    for vertex in vertices.iter().skip(1) {
        min_x = min_x.min(vertex.position[0]);
        max_x = max_x.max(vertex.position[0]);
        min_y = min_y.min(vertex.position[1]);
        max_y = max_y.max(vertex.position[1]);
    }
    let x = min_x.floor().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    let y = min_y.floor().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    let right = max_x.ceil().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    let bottom = max_y.ceil().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    RectI16 {
        x,
        y,
        w: right.saturating_sub(x),
        h: bottom.saturating_sub(y),
    }
}

pub(crate) fn render_motion_to_host<B>(
    backend: &mut B,
    cache: &mut HostPrimRenderCache,
    motion: &MotionManager,
    virtual_size: (u32, u32),
) -> RfvpResult<RenderFrame>
where
    B: RenderBackend<Error = RfvpError> + TextureBackend<Error = RfvpError>,
{
    backend.begin_frame(
        u16::try_from(virtual_size.0).map_err(|_| {
            log::error!("virtual size {virtual_size:?} is too large");
            RfvpError::CapacityExceeded
        })?,
        u16::try_from(virtual_size.1).map_err(|_| RfvpError::CapacityExceeded)?,
    )?;
    cache.ensure_white(backend)?;

    let prim_manager = motion.prim_manager();
    let graphs = motion.graphs();
    let snow_motions = motion.snow_motions();

    let tree_started = crate::perf::start();
    let mut frame = RenderFrame::default();
    let mut order = 0u32;
    let mut visit = vec![0u8; 4096];
    collect_tree(
        backend,
        cache,
        &mut frame.commands,
        &mut frame.hit_proxies,
        &mut order,
        virtual_size,
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

    // Between the two trees, as the desktop renderer does: the scene dissolve (a full-screen
    // color fade), then dissolve2 (the engine's own fades around save, load and transitions).
    // Mask dissolves are not drawn yet, on any platform.
    let dissolve_alpha = match motion.get_dissolve_type() {
        DissolveType::Static | DissolveType::ColoredFadeIn | DissolveType::ColoredFadeOut => {
            Some((motion.get_dissolve_color_id(), motion.get_dissolve_alpha()))
        }
        _ => None,
    };
    let fades = [
        dissolve_alpha,
        Some((motion.get_dissolve2_color_id(), motion.get_dissolve2_alpha())),
    ];
    for (color_id, alpha) in fades.into_iter().flatten() {
        if alpha <= 0.0 {
            continue;
        }
        let c = motion.color_manager.get_entry(color_id as u8);
        let color = vec4(
            c.get_r() as f32 / 255.0,
            c.get_g() as f32 / 255.0,
            c.get_b() as f32 / 255.0,
            (c.get_a() as f32 / 255.0) * alpha,
        );
        emit_sprite(
            &mut frame.commands,
            &mut frame.hit_proxies,
            &mut order,
            -1,
            Mat4::IDENTITY,
            virtual_size.0 as f32,
            virtual_size.1 as f32,
            vec2(0.5, 0.5),
            vec2(0.5, 0.5),
            color,
            DrawTextureKey::White,
            CommandBlendMode::Normal,
        )?;
    }

    let root = prim_manager.get_custom_root_prim_id() as i16;
    if root != 0 {
        let mut visit = vec![0u8; 4096];
        collect_tree(
            backend,
            cache,
            &mut frame.commands,
            &mut frame.hit_proxies,
            &mut order,
            virtual_size,
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

    crate::perf::stop(11, tree_started);
    let t = crate::perf::start();
    backend.submit_commands(&frame.commands)?;
    crate::perf::stop(12, t);
    let t = crate::perf::start();
    backend.end_frame()?;
    crate::perf::stop(13, t);
    Ok(frame)
}

#[allow(clippy::too_many_arguments)]
fn collect_tree<B>(
    backend: &mut B,
    cache: &mut HostPrimRenderCache,
    commands: &mut Vec<RenderCommand>,
    hit_proxies: &mut HitProxyTable,
    order: &mut u32,
    virtual_size: (u32, u32),
    prim_manager: &PrimManager,
    color_manager: &ColorManager,
    graphs: &crate::subsystem::resources::graph_buff::Graphs,
    snow_motions: &[SnowMotion],
    v3d_x: i32,
    v3d_y: i32,
    v3d_z: i32,
    prim_id: i16,
    parent_x: f32,
    parent_y: f32,
    visit: &mut [u8],
    depth: usize,
) -> RfvpResult<()>
where
    B: TextureBackend<Error = RfvpError>,
{
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
    // A sprite chain that loops back on itself must not hang the frame.
    let mut sprt_steps = 0u32;
    while sprt != -1 {
        sprt_steps += 1;
        if sprt_steps > 4096 {
            visit[prim_idx] = 2;
            return Ok(());
        }
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
        PrimType::PrimTypeSprt => {
            let tex_id = draw_prim.get_texture_id();
            let graph_id = if tex_id >= 0 {
                Some(tex_id as u16)
            } else if tex_id == -2 {
                Some(crate::subsystem::resources::videoplayer::MOVIE_GRAPH_ID)
            } else {
                None
            };
            if let Some(graph_id) = graph_id {
                if let Some(graph) = graphs.get(graph_id as usize) {
                    if cache.upload_graph_if_needed(backend, graph_id, graph)? {
                        emit_graph_sprite(
                            commands,
                            hit_proxies,
                            order,
                            virtual_size,
                            graph_id,
                            graph,
                            &draw_prim,
                            parent_x,
                            parent_y,
                            draw_x,
                            draw_y,
                            draw_alpha,
                            v3d_x,
                            v3d_y,
                            v3d_z,
                            false,
                            draw_id,
                        )?;
                    }
                }
            }
        }
        PrimType::PrimTypeText => {
            let slot = draw_prim.get_text_index();
            if (0..=31).contains(&slot) {
                let graph_id = 4064u16 + slot as u16;
                if let Some(graph) = graphs.get(graph_id as usize) {
                    if cache.upload_graph_if_needed(backend, graph_id, graph)? {
                        emit_graph_sprite(
                            commands,
                            hit_proxies,
                            order,
                            virtual_size,
                            graph_id,
                            graph,
                            &draw_prim,
                            parent_x,
                            parent_y,
                            draw_x,
                            draw_y,
                            draw_alpha,
                            v3d_x,
                            v3d_y,
                            v3d_z,
                            true,
                            draw_id,
                        )?;
                    }
                }
            }
        }
        PrimType::PrimTypeSnow => {
            emit_snow(
                backend,
                cache,
                commands,
                hit_proxies,
                order,
                virtual_size,
                graphs,
                snow_motions,
                &draw_prim,
                parent_x,
                parent_y,
                draw_x,
                draw_y,
                draw_alpha,
            )?;
        }
        PrimType::PrimTypeTile => {
            let w = draw_prim.get_w() as f32;
            let h = draw_prim.get_h() as f32;
            let color_id = draw_prim.get_tile();
            let color = color_manager.get_entry(color_id as u8);
            if w > 0.0 && h > 0.0 {
                let rgba = vec4(
                    color.get_r() as f32 / 255.0,
                    color.get_g() as f32 / 255.0,
                    color.get_b() as f32 / 255.0,
                    draw_alpha * (color.get_a() as f32 / 255.0),
                );
                // The original engine's draw_color_tile() uses only the
                // accumulated parent position plus the tile's X/Y and W/H.
                // Tile primitives do not apply rotation, scale, pivot, or V3D.
                let model = Mat4::from_translation(vec3(
                    parent_x + draw_x,
                    parent_y + draw_y,
                    0.0,
                ));
                emit_sprite(
                    commands,
                    hit_proxies,
                    order,
                    draw_id,
                    model,
                    w,
                    h,
                    // The original engine draws Tile primitives as an untextured
                    // diffuse-color quad. Keep the compatibility 1x1 white texture, but
                    // sample its center at every vertex so filtering can never pull an
                    // edge/border value into the solid rectangle.
                    vec2(0.5, 0.5),
                    vec2(0.5, 0.5),
                    rgba,
                    DrawTextureKey::White,
                    prim_blend(&draw_prim),
                )?;
            }
        }
    }

    let mut children = Vec::new();
    let mut child = first_child;
    let mut steps = 0usize;
    while child != -1 {
        if steps >= 4096 || child < 0 || child >= 4096 {
            break;
        }
        steps += 1;
        children.push(child);
        let child_prim = prim_manager.get_prim_immutable(child);
        child = child_prim.get_next_sibling_idx();
    }

    let next_parent_x = parent_x + draw_x;
    let next_parent_y = parent_y + draw_y;
    for child in children {
        collect_tree(
            backend,
            cache,
            commands,
            hit_proxies,
            order,
            virtual_size,
            prim_manager,
            color_manager,
            graphs,
            snow_motions,
            v3d_x,
            v3d_y,
            v3d_z,
            child,
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
fn emit_graph_sprite(
    commands: &mut Vec<RenderCommand>,
    hit_proxies: &mut HitProxyTable,
    order: &mut u32,
    virtual_size: (u32, u32),
    graph_id: u16,
    graph: &GraphBuff,
    prim: &Prim,
    parent_x: f32,
    parent_y: f32,
    draw_x: f32,
    draw_y: f32,
    draw_alpha: f32,
    v3d_x: i32,
    v3d_y: i32,
    v3d_z: i32,
    text_graph: bool,
    prim_id: i16,
) -> RfvpResult<()> {
    let Some((tw, th)) = graph.image_dimensions() else {
        return Ok(());
    };
    if tw == 0 || th == 0 {
        return Ok(());
    }
    let attr = prim.get_attr();
    let use_rect = (attr & 1) != 0;
    let mut text_draw_x = 0.0;
    let mut text_draw_y = 0.0;

    let (mut w, mut h, mut u, mut v, mut tex_w, mut tex_h) = if text_graph {
        let Some(region) = graph.text_texture_region(
            tw,
            th,
            use_rect,
            prim.get_u() as f32,
            prim.get_v() as f32,
            prim.get_w() as f32,
            prim.get_h() as f32,
        ) else {
            return Ok(());
        };
        text_draw_x = region.draw_x;
        text_draw_y = region.draw_y;
        (
            region.draw_w,
            region.draw_h,
            region.tex_x,
            region.tex_y,
            region.tex_w,
            region.tex_h,
        )
    } else if use_rect {
        let mut w = prim.get_w() as f32;
        let mut h = prim.get_h() as f32;
        if w <= 0.0 {
            w = graph.get_width() as f32;
        }
        if h <= 0.0 {
            h = graph.get_height() as f32;
        }
        (
            w,
            h,
            prim.get_u() as f32 - graph.get_offset_x() as f32,
            prim.get_v() as f32 - graph.get_offset_y() as f32,
            w,
            h,
        )
    } else {
        (
            graph.get_width() as f32,
            graph.get_height() as f32,
            0.0,
            0.0,
            graph.get_width() as f32,
            graph.get_height() as f32,
        )
    };

    let mut clip_x = 0.0;
    let mut clip_y = 0.0;
    if use_rect && !text_graph {
        if u < 0.0 {
            clip_x = -u;
            w += u;
            tex_w += u;
            u = 0.0;
        }
        if v < 0.0 {
            clip_y = -v;
            h += v;
            tex_h += v;
            v = 0.0;
        }
        let max_w = tw as f32 - u;
        let max_h = th as f32 - v;
        w = w.min(max_w);
        h = h.min(max_h);
        tex_w = tex_w.min(max_w);
        tex_h = tex_h.min(max_h);
    }

    if w <= 0.0 || h <= 0.0 || tex_w <= 0.0 || tex_h <= 0.0 {
        return Ok(());
    }

    let uv0 = vec2(u / tw as f32, v / th as f32);
    let uv1 = vec2((u + tex_w) / tw as f32, (v + tex_h) / th as f32);
    let color = vec4(1.0, 1.0, 1.0, draw_alpha);
    let off_x = graph.get_offset_x() as f32
        + if text_graph { text_draw_x } else { clip_x };
    let off_y = graph.get_offset_y() as f32
        + if text_graph { text_draw_y } else { clip_y };
    let (pivot_x, pivot_y) = if (attr & 2) != 0 {
        (prim.get_opx() as f32, prim.get_opy() as f32)
    } else {
        (graph.get_u() as f32, graph.get_v() as f32)
    };
    let model = build_draw_model(
        virtual_size,
        prim,
        parent_x,
        parent_y,
        draw_x,
        draw_y,
        off_x,
        off_y,
        pivot_x,
        pivot_y,
        v3d_x,
        v3d_y,
        v3d_z,
    );

    emit_sprite(
        commands,
        hit_proxies,
        order,
        prim_id,
        model,
        w,
        h,
        uv0,
        uv1,
        color,
        DrawTextureKey::Graph(graph_id),
        prim_blend(prim),
    )
}

#[allow(clippy::too_many_arguments)]
fn emit_snow<B>(
    backend: &mut B,
    cache: &mut HostPrimRenderCache,
    commands: &mut Vec<RenderCommand>,
    hit_proxies: &mut HitProxyTable,
    order: &mut u32,
    virtual_size: (u32, u32),
    graphs: &crate::subsystem::resources::graph_buff::Graphs,
    snow_motions: &[SnowMotion],
    prim: &Prim,
    parent_x: f32,
    parent_y: f32,
    draw_x: f32,
    draw_y: f32,
    draw_alpha: f32,
) -> RfvpResult<()>
where
    B: TextureBackend<Error = RfvpError>,
{
    let snow_id = prim.get_texture_id();
    if snow_id < 0 {
        return Ok(());
    }
    let Some(sm) = snow_motions.get(snow_id as usize) else {
        return Ok(());
    };
    if !sm.enabled || sm.flake_count <= 0 || sm.texture_id < 0 || sm.flake_w <= 1 || sm.flake_h <= 1
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
    let variant_count = sm.variant_count.max(1) as u32;
    let tile_w_cfg = (sm.flake_w - 1) as f32;
    let tile_h_cfg = (sm.flake_h - 1) as f32;
    let count = sm.flake_count.max(0).min(1024) as usize;

    for idx in 0..count {
        let flake_idx = sm.flake_ptrs[idx];
        let flake = &sm.flakes[flake_idx];
        let graph_i32 = base_tex + (flake.variant_idx % variant_count) as i32;
        if graph_i32 < 0 {
            continue;
        }
        let graph_id = graph_i32 as u16;
        let Some(graph) = graphs.get(graph_id as usize) else {
            continue;
        };
        if !cache.upload_graph_if_needed(backend, graph_id, graph)? {
            continue;
        }
        let Some((tw, th)) = graph.image_dimensions() else {
            continue;
        };
        if tw == 0 || th == 0 {
            continue;
        }

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
        emit_sprite(
            commands,
            hit_proxies,
            order,
            prim.get_texture_id() as i16,
            model,
            w,
            h,
            vec2(0.0, 0.0),
            vec2(tile_w / tw as f32, tile_h / th as f32),
            vec4(1.0, 1.0, 1.0, alpha),
            DrawTextureKey::Graph(graph_id),
            prim_blend(prim),
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    #[test]
    fn shared_edges_preserve_rotated_and_projected_quads() {
        for attr in [0, 4] {
            for angle in [0, 450, -900, 1800] {
                let mut prim = Prim::new();
                prim.set_attr(attr);
                prim.set_rotation(angle);
                prim.set_factor_x(1250);
                prim.set_factor_y(-800);
                prim.set_z(1000);
                let model = build_draw_model((1920, 1080), &prim,
                    35.0, -20.0, 250.0, 105.0, 18.0, -9.0, 12.0, 24.0, 10, 20, 50);
                let corners = sprite_corners(model, 320.0, 240.0);
                for (actual, point) in corners.into_iter().zip([
                    vec3(0.0, 240.0, 0.0), vec3(0.0, 0.0, 0.0),
                    vec3(320.0, 240.0, 0.0), vec3(320.0, 0.0, 0.0)]) {
                    let delta = actual - model.transform_point3(point);
                    assert!(delta.abs().max_element() < 0.001);
                }
            }
        }
    }

    #[test]
    fn unrotated_models_preserve_scaled_pivots_and_vertices() {
        for (fx, fy) in [(1000, 1000), (1250, 800), (-1000, 0), (0, -250)] {
            let mut prim = Prim::new();
            prim.set_factor_x(fx);
            prim.set_factor_y(fy);
            let actual = build_draw_model((1920, 1080), &prim,
                35.0, -20.0, 250.0, 105.0, 18.0, -9.0, 12.0, 24.0, 0, 0, 0);
            let expected = Mat4::from_translation(vec3(297.0, 109.0, 0.0))
                * Mat4::from_scale(vec3(fx as f32 / 1000.0, fy as f32 / 1000.0, 1.0))
                * Mat4::from_rotation_z(0.0)
                * Mat4::from_translation(vec3(6.0, -33.0, 0.0));
            for point in [vec3(0.0, 0.0, 0.0), vec3(320.0, 0.0, 0.0),
                          vec3(0.0, 240.0, 0.0), vec3(320.0, 240.0, 0.0)] {
                let delta = actual.transform_point3(point) - expected.transform_point3(point);
                assert!(delta.abs().max_element() < 0.001);
            }
        }
    }
}
