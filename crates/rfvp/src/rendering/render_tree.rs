use egui::{epaint as ept, *};
use std::collections::HashMap;

// ------------------------- Math / Transform -------------------------


#[derive(Clone, Copy, Debug, Default)]
pub struct Transform2D {
    pub position: Vec2, // screen-space, px
    pub scale: Vec2, // per-axis scale
    pub rotation: f32, // radians, clockwise (screen y-down)
    pub pivot: Vec2, // pivot in local px coordinates
}

impl Transform2D {
    pub fn new(position: Vec2) -> Self {
        Self { position, scale: Vec2::splat(1.0), rotation: 0.0, pivot: Vec2::ZERO }
    }

    pub fn to_affine(&self) -> Affine2 {
        // Affine = T(pos) * T(pivot) * R(rot) * S(scale) * T(-pivot)
        Affine2::translate(self.position)
            * Affine2::translate(self.pivot)
            * Affine2::rotate(self.rotation)
            * Affine2::scale(self.scale)
            * Affine2::translate(-self.pivot)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Affine2 {
    // 2x3 affine matrix: [ m11 m12 | tx ]
    //                    [ m21 m22 | ty ]
    pub m11: f32,
    pub m12: f32,
    pub m21: f32,
    pub m22: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Affine2 {
    pub fn identity() -> Self {
        Self { m11: 1.0, m12: 0.0, m21: 0.0, m22: 1.0, tx: 0.0, ty: 0.0 }
    }
    pub fn translate(v: Vec2) -> Self {
        Self { tx: v.x, ty: v.y, ..Self::identity() }
    }
    pub fn scale(s: Vec2) -> Self {
        Self { m11: s.x, m22: s.y, ..Self::identity() }
    }
    pub fn rotate(rad: f32) -> Self {
        // screen y-down → clockwise rotation has positive angle
        let c = rad.cos();
        let s = rad.sin();
        // standard 2D rotation matrix (assuming y-up) is [c -s; s c]
        // For y-down screen space, this still gives the desired visual CW rotation.
        Self { m11: c, m12: -s, m21: s, m22: c, ..Self::identity() }
    }
    pub fn transform_pos(&self, p: Pos2) -> Pos2 {
        Pos2::new(
            self.m11 * p.x + self.m12 * p.y + self.tx,
            self.m21 * p.x + self.m22 * p.y + self.ty,
        )
    }
}

use std::ops::Mul;
impl Mul for Affine2 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        // self * rhs
        let m11 = self.m11 * rhs.m11 + self.m12 * rhs.m21;
        let m12 = self.m11 * rhs.m12 + self.m12 * rhs.m22;
        let m21 = self.m21 * rhs.m11 + self.m22 * rhs.m21;
        let m22 = self.m21 * rhs.m12 + self.m22 * rhs.m22;
        let tx = self.m11 * rhs.tx + self.m12 * rhs.ty + self.tx;
        let ty = self.m21 * rhs.tx + self.m22 * rhs.ty + self.ty;
        Self { m11, m12, m21, m22, tx, ty }
    }
}


// ------------------------- Easing / Animation -------------------------

#[derive(Clone, Copy, Debug)]
pub enum Easing {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
}

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseInQuad => t * t,
            Easing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOutQuad => if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 },
            Easing::EaseInCubic => t * t * t,
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOutCubic => if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RepeatMode { Once, Loop, PingPong }

pub trait Lerp: Copy {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
}
impl Lerp for f32 { fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t } }
impl Lerp for Vec2 { fn lerp(a: Vec2, b: Vec2, t: f32) -> Vec2 { a + (b - a) * t } }

#[derive(Clone, Copy, Debug)]
pub struct AnimTrack<T: Lerp + Copy> {
    pub start_time: f64,
    pub duration: f64,
    pub from: T,
    pub to: T,
    pub easing: Easing,
    pub repeat: RepeatMode,
    pub enabled: bool,
}
impl<T: Lerp + Copy> AnimTrack<T> {
    pub fn value_at(&self, now: f64) -> (T, bool) {
        if !self.enabled || self.duration <= 0.0 {
            return (self.to, true);
        }
        let mut t = ((now - self.start_time) / self.duration) as f32;
        let finished;
        let t_mod = match self.repeat {
            RepeatMode::Once => { finished = t >= 1.0; t.clamp(0.0, 1.0) }
            RepeatMode::Loop => { finished = false; t.fract().abs() }
            RepeatMode::PingPong => {
                finished = false;
                let w = t.fract().abs();
                if ((t.floor() as i64) & 1) == 0 { w } else { 1.0 - w }
            }
        };
        let eased = self.easing.apply(t_mod);
        (T::lerp(self.from, self.to, eased), finished)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PrimitiveAnimations {
    pub pos: Option<AnimTrack<Vec2>>,
    pub scale: Option<AnimTrack<Vec2>>,
    pub rot: Option<AnimTrack<f32>>,
    pub opacity: Option<AnimTrack<f32>>, // 0..1, multiplies color alpha
}

impl PrimitiveAnimations {
    pub fn apply(&self, base: &mut Transform2D, base_alpha: &mut f32, now: f64) {
        if let Some(tr) = &self.pos { base.position = tr.value_at(now).0; }
        if let Some(sc) = &self.scale { base.scale = sc.value_at(now).0; }
        if let Some(rt) = &self.rot { base.rotation = rt.value_at(now).0; }
        if let Some(op) = &self.opacity { *base_alpha = op.value_at(now).0; }
    }
}

// ------------------------- Primitives -------------------------

#[derive(Clone, Debug)]
pub enum PrimitiveKind {
    Image(ImagePrimitive),
    ColorRect(ColorRectPrimitive),
    Text(TextPrimitive),
    Video(VideoPrimitive),
    Particles(ParticlePrimitive),
}

impl Default for PrimitiveKind {
    fn default() -> Self {
        PrimitiveKind::ColorRect(ColorRectPrimitive { size: Vec2::splat(100.0), color: Color32::WHITE, corner_radius: 0.0 })
    }
}

#[derive(Clone, Debug, Default)]
pub struct PrimitiveInstance {
    pub kind: PrimitiveKind,
    pub local: Transform2D, // local (relative to owning Node)
    pub z: f32,             // z-offset relative to node z
    pub visible: bool,
    pub alpha: f32,         // 0..1 multiplier
    pub anim: PrimitiveAnimations,
}

#[derive(Clone, Debug)]
pub struct ImagePrimitive {
    pub image_index: u32,   // resource index
    pub size: Vec2,         // local size in px
    pub uv_min: Vec2,       // normalized
    pub uv_max: Vec2,       // normalized
    pub tint: Color32,      // multiplies sampled color
}

#[derive(Clone, Debug)]
pub struct ColorRectPrimitive {
    pub size: Vec2,
    pub color: Color32,
    pub corner_radius: f32,
}

#[derive(Clone, Debug)]
pub struct TextPrimitive {
    pub text: String,
    pub font: FontId,
    pub color: Color32,
    pub wrap_width: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct VideoPrimitive {
    pub video_index: u32,   // resource index
    pub size: Vec2,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    pub tint: Color32,
}

#[derive(Clone, Debug)]
pub struct ParticlePrimitive {
    pub emitter: ParticleEmitter,
}

// ------------------------- Node / Scene Graph -------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct NodeId(pub u64);

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub local: Transform2D,
    pub z: f32,
    pub visible: bool,
    pub primitives: Vec<PrimitiveInstance>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(name: impl Into<String>) -> Self {
        Self { id: NodeId(0), name: name.into(), ..Default::default() }
    }
}

#[derive(Default)]
pub struct RenderTree {
    pub root: Node,
    next_id: u64,
}

impl RenderTree {
    pub fn new() -> Self { Self { root: Node::new("root"), next_id: 1 } }
    pub fn alloc_id(&mut self) -> NodeId { let id = self.next_id; self.next_id += 1; NodeId(id) }

    pub fn assign_ids_recursive(&mut self) { Self::assign_ids(&mut self.root, &mut self.next_id); }
    fn assign_ids(node: &mut Node, next: &mut u64) {
        node.id = NodeId(*next); *next += 1;
        for ch in &mut node.children { Self::assign_ids(ch, next); }
    }
}

// ------------------------- Resources -------------------------

pub trait VideoSource {
    /// Update internal clock by `dt` seconds, returning the next RGBA frame if changed.
    fn update(&mut self, dt: f32) -> Option<VideoFrame>;
    fn size(&self) -> [u32; 2];
}

pub struct VideoFrame {
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

pub struct Resources {
    images: HashMap<u32, TextureHandle>,
    image_tex_id: HashMap<u32, TextureId>,
    videos: HashMap<u32, (Box<dyn VideoSource>, TextureHandle)>,
}

impl Resources {
    pub fn new() -> Self { Self { images: HashMap::new(), image_tex_id: HashMap::new(), videos: HashMap::new() } }

    pub fn set_image(&mut self, ctx: &Context, index: u32, img: ColorImage) {
        let handle = ctx.load_texture(format!("img_{index}"), img, TextureOptions::LINEAR);
        let tid = handle.id();
        self.images.insert(index, handle);
        self.image_tex_id.insert(index, tid);
    }

    pub fn image_tex_id(&self, index: u32) -> Option<TextureId> { self.image_tex_id.get(&index).copied() }

    pub fn set_video(&mut self, ctx: &Context, index: u32, mut src: Box<dyn VideoSource>) {
        let [w, h] = src.size();
        let blank = ColorImage::new([w as usize, h as usize], Color32::TRANSPARENT);
        let handle = ctx.load_texture(format!("vid_{index}"), blank, TextureOptions::LINEAR);
        self.videos.insert(index, (src, handle));
    }

    /// Call once per frame to advance video textures.
    pub fn update_videos(&mut self, dt: f32) {
        for (_idx, (src, tex)) in self.videos.iter_mut() {
            if let Some(frame) = src.update(dt) {
                let color_image = ept::image::ColorImage { size: [frame.width, frame.height], pixels: frame.rgba.chunks_exact(4).map(|abgr| {
                            // Our frame is RGBA; egui expects sRGBA in Color32 ordering
                            Color32::from_rgba_unmultiplied(abgr[0], abgr[1], abgr[2], abgr[3])
                        }).collect::<Vec<Color32>>() };

                tex.set(
                    color_image,
                    TextureOptions::default()
                );
            }
        }
    }

    pub fn video_tex_id(&self, index: u32) -> Option<TextureId> {
        self.videos.get(&index).map(|(_, h)| h.id())
    }
}

// ------------------------- Particles -------------------------

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,     // remaining secs
    pub size: Vec2,
    pub color: Color32,
}

#[derive(Clone, Debug)]
pub struct ParticleEmitterSpec {
    pub spawn_rate: f32, // particles per second
    pub lifetime: std::ops::Range<f32>,
    pub speed: std::ops::Range<f32>,
    pub angle_deg: std::ops::Range<f32>, // emission cone center-around local +X (0°) in screen-space
    pub size: std::ops::Range<f32>,
    pub color: Color32,
    pub texture_index: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ParticleEmitter {
    pub spec: ParticleEmitterSpec,
    pub particles: Vec<Particle>,
    pub acc: f32,
}

impl ParticleEmitter {
    pub fn new(spec: ParticleEmitterSpec) -> Self { Self { spec, particles: Vec::new(), acc: 0.0 } }

    pub fn update(&mut self, dt: f32, local_to_world: &Affine2) {
        use rand::{Rng, rngs::SmallRng, SeedableRng};
        static mut RNG: Option<SmallRng> = None;
        let rng = unsafe { RNG.get_or_insert_with(|| SmallRng::from_entropy()) };
        let mut rng = rng.clone();

        // Spawn
        self.acc += self.spec.spawn_rate * dt;
        let to_spawn = self.acc.floor() as i32;
        self.acc -= to_spawn as f32;
        for _ in 0..to_spawn.max(0) {
            let life = rng.gen_range(self.spec.lifetime.clone());
            let speed = rng.gen_range(self.spec.speed.clone());
            let angle = rng.gen_range(self.spec.angle_deg.clone()) * std::f32::consts::PI / 180.0;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let size = rng.gen_range(self.spec.size.clone());
            let p = Particle {
                pos: Vec2::ZERO,
                vel: dir * speed,
                life,
                size: Vec2::splat(size),
                color: self.spec.color,
            };
            self.particles.push(p);
        }

        // Integrate
        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            p.life -= dt;
            if p.life <= 0.0 { self.particles.swap_remove(i); continue; }
            p.pos += p.vel * dt;
            i += 1;
        }

        // local_to_world is used at render time; we keep particles in emitter-local space here.
    }
}


// ------------------------- Drawable Flattening -------------------------

#[derive(Clone, Debug)]
pub enum DrawableKind {
    Mesh { mesh: ept::Mesh, tex: Option<TextureId> },
    Text { pos: Pos2, text: String, font: FontId, color: Color32, wrap_width: Option<f32> },
}

#[derive(Clone, Debug)]
pub struct Drawable { pub z: f32, pub kind: DrawableKind }

fn quad_to_mesh(corners: [Pos2; 4], color: Color32) -> ept::Mesh {
    // 2 triangles (0,1,2) (0,2,3)
    let mut m = ept::Mesh::default();
    let base = 0u32;
    m.vertices.push(ept::Vertex { pos: corners[0], uv: egui::pos2(0.0, 0.0), color });
    m.vertices.push(ept::Vertex { pos: corners[1], uv: egui::pos2(1.0, 0.0), color });
    m.vertices.push(ept::Vertex { pos: corners[2], uv: egui::pos2(1.0, 1.0), color });
    m.vertices.push(ept::Vertex { pos: corners[3], uv: egui::pos2(0.0, 1.0), color });
    m.indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
    m
}

fn textured_quad_to_mesh(corners: [Pos2; 4], uv: [Pos2; 4], color: Color32, tid: TextureId) -> ept::Mesh {
    let mut m = ept::Mesh::default();
    m.texture_id = tid;
    let base = 0u32;
    m.vertices.push(ept::Vertex { pos: corners[0], uv: uv[0], color });
    m.vertices.push(ept::Vertex { pos: corners[1], uv: uv[1], color });
    m.vertices.push(ept::Vertex { pos: corners[2], uv: uv[2], color });
    m.vertices.push(ept::Vertex { pos: corners[3], uv: uv[3], color });
    m.indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
    m
}

#[derive(Clone, Copy, Debug)]
pub struct Aabb { pub min: Pos2, pub max: Pos2 }
impl Aabb { pub fn intersects(&self, r: Rect) -> bool { self.max.x >= r.min.x && self.min.x <= r.max.x && self.max.y >= r.min.y && self.min.y <= r.max.y } }

fn transformed_quad(local_size: Vec2, world: &Affine2, pivot: Vec2) -> [Pos2; 4] {
    // local rect (0,0)-(w,h) with pivot (px,py)
    let p0 = Pos2::new(0.0, 0.0);
    let p1 = Pos2::new(local_size.x, 0.0);
    let p2 = Pos2::new(local_size.x, local_size.y);
    let p3 = Pos2::new(0.0, local_size.y);
    let t = Transform2D { position: Vec2::ZERO, scale: Vec2::splat(1.0), rotation: 0.0, pivot };
    let pre = t.to_affine();
    [
        world.transform_pos(pre.transform_pos(p0)),
        world.transform_pos(pre.transform_pos(p1)),
        world.transform_pos(pre.transform_pos(p2)),
        world.transform_pos(pre.transform_pos(p3)),
    ]
}

fn corners_aabb(cs: &[Pos2; 4]) -> Aabb {
    let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for c in cs { min.x = min.x.min(c.x); min.y = min.y.min(c.y); max.x = max.x.max(c.x); max.y = max.y.max(c.y); }
    Aabb { min, max }
}

pub struct FlattenCtx {
    pub drawables: Vec<Drawable>,
    pub viewport: Rect,
}

fn push_drawable(ctx: &mut FlattenCtx, z: f32, kind: DrawableKind) {
    ctx.drawables.push(Drawable { z, kind });
}

fn flatten_node(node: &Node, parent_world: Affine2, parent_z: f32, ctx: &mut FlattenCtx, res: &Resources, now: f64, dt: f32) {
    if !node.visible { return; }
    let world = parent_world * node.local.to_affine();
    let base_z = parent_z + node.z;

    for prim in &node.primitives {
        if !prim.visible { continue; }
        let mut t = prim.local;
        let mut alpha = prim.alpha;
        prim.anim.apply(&mut t, &mut alpha, now);
        let world2 = world * t.to_affine();
        let z = base_z + prim.z;

        match &prim.kind {
            PrimitiveKind::ColorRect(cr) => {
                let corners = transformed_quad(cr.size, &world2, t.pivot);
                let aabb = corners_aabb(&corners);
                if aabb.intersects(ctx.viewport) {
                    // Use a solid-color mesh; vertex color takes precedence
                    let color = multiply_alpha(cr.color, alpha);
                    let mesh = quad_to_mesh(corners, color);
                    push_drawable(ctx, z, DrawableKind::Mesh { mesh, tex: None });
                }
            }
            PrimitiveKind::Image(img) => {
                if let Some(tid) = res.image_tex_id(img.image_index) {
                    let corners = transformed_quad(img.size, &world2, t.pivot);
                    let aabb = corners_aabb(&corners);
                    if aabb.intersects(ctx.viewport) {
                        let uv = [
                            Pos2::new(img.uv_min.x, img.uv_min.y),
                            Pos2::new(img.uv_max.x, img.uv_min.y),
                            Pos2::new(img.uv_max.x, img.uv_max.y),
                            Pos2::new(img.uv_min.x, img.uv_max.y),
                        ];
                        let color = multiply_alpha(img.tint, alpha);
                        let mesh = textured_quad_to_mesh(corners, uv, color, tid);
                        push_drawable(ctx, z, DrawableKind::Mesh { mesh, tex: Some(tid) });
                    }
                }
            }
            PrimitiveKind::Video(v) => {
                if let Some(tid) = res.video_tex_id(v.video_index) {
                    let corners = transformed_quad(v.size, &world2, t.pivot);
                    let aabb = corners_aabb(&corners);
                    if aabb.intersects(ctx.viewport) {
                        let uv = [
                            Pos2::new(v.uv_min.x, v.uv_min.y),
                            Pos2::new(v.uv_max.x, v.uv_min.y),
                            Pos2::new(v.uv_max.x, v.uv_max.y),
                            Pos2::new(v.uv_min.x, v.uv_max.y),
                        ];
                        let color = multiply_alpha(v.tint, alpha);
                        let mesh = textured_quad_to_mesh(corners, uv, color, tid);
                        push_drawable(ctx, z, DrawableKind::Mesh { mesh, tex: Some(tid) });
                    }
                }
            }
            PrimitiveKind::Text(tx) => {
                // No rotation support for text here
                let world_pos = world2.transform_pos(Pos2::new(0.0, 0.0));
                let aabb = Aabb { min: world_pos, max: world_pos + tx_estimate_aabb(tx, t) };
                if aabb.intersects(ctx.viewport) {
                    let color = multiply_alpha(tx.color, alpha);
                    push_drawable(ctx, z, DrawableKind::Text { pos: world_pos, text: tx.text.clone(), font: tx.font.clone(), color, wrap_width: tx.wrap_width });
                }
            }
            PrimitiveKind::Particles(pp) => {
                // Update particles with dt, then draw quads
                let mut em = pp.emitter.clone();
                em.update(dt, &world2);
                for p in em.particles.iter() {
                    let corners = transformed_quad(p.size, &(world2 * Affine2::translate(p.pos)), Vec2::splat(0.5) * p.size);
                    let aabb = corners_aabb(&corners);
                    if aabb.intersects(ctx.viewport) {
                        let mesh = quad_to_mesh(corners, p.color);
                        push_drawable(ctx, z, DrawableKind::Mesh { mesh, tex: None });
                    }
                }
            }
        }
    }

    for ch in &node.children { flatten_node(ch, world, base_z, ctx, res, now, dt); }
}

fn multiply_alpha(mut c: Color32, alpha: f32) -> Color32 {
    let a = ((c.a() as f32) * alpha).clamp(0.0, 255.0) as u8;
    c = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
    c
}

fn tx_estimate_aabb(tx: &TextPrimitive, _t: Transform2D) -> Vec2 {
    // rough estimate: height comes from font size; width from either wrap or text len * factor
    let h = tx.font.size;
    let w = if let Some(wrap) = tx.wrap_width { wrap } else { tx.text.len() as f32 * (tx.font.size * 0.6) };
    Vec2::new(w, h)
}

// ------------------------- Renderer API -------------------------

pub struct Renderer {
    pub resources: Resources,
    pub viewport: Rect,
}

impl Renderer {
    pub fn new(viewport: Rect) -> Self { Self { resources: Resources::new(), viewport } }

    pub fn update_and_draw(&mut self, painter: &Painter, tree: &RenderTree, now: f64, dt: f32) {
        // Update frame-based resources
        self.resources.update_videos(dt);

        // Flatten
        let mut fctx = FlattenCtx { drawables: Vec::new(), viewport: self.viewport };
        flatten_node(&tree.root, Affine2::identity(), 0.0, &mut fctx, &self.resources, now, dt);

        // Sort by z, stable by insertion order
        fctx.drawables.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap_or(std::cmp::Ordering::Equal));

        // Emit egui shapes
        for d in fctx.drawables {
            match d.kind {
                DrawableKind::Mesh { mesh, .. } => { painter.add(ept::Shape::mesh(mesh)); }
                DrawableKind::Text { pos, text, font, color, wrap_width } => {
                    painter.text(pos, Align2::LEFT_TOP, text, font, color);
                    // if let Some(_wrap) = wrap_width { 

                    // }
                },
            }
        }
    }
}

// ------------------------- Example: eframe app -------------------------

#[cfg(feature = "example")] 
pub mod example {
    use super::*;

    pub struct DemoApp {
        pub renderer: Renderer,
        pub tree: RenderTree,
        pub start: std::time::Instant,
    }

    impl DemoApp {
        pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
            let screen = Rect::from_min_size(Pos2::ZERO, cc.egui_ctx.input(|i| i.screen_rect().size()));
            let mut renderer = Renderer::new(screen);

            // Add an image resource at index 1
            let checker = make_checker_image(256, 256);
            renderer.resources.set_image(&cc.egui_ctx, 1, checker);

            // Build a small scene
            let mut tree = RenderTree::new();
            tree.root.visible = true;

            let mut node = Node::new("image_node");
            node.local = Transform2D { position: vec2(200.0, 200.0), scale: vec2(1.0, 1.0), rotation: 0.0, pivot: vec2(64.0, 64.0) };
            node.z = 0.5;

            // Image primitive with animation
            let mut img_inst = PrimitiveInstance {
                kind: PrimitiveKind::Image(ImagePrimitive {
                    image_index: 1,
                    size: vec2(128.0, 128.0),
                    uv_min: vec2(0.0, 0.0), uv_max: vec2(1.0, 1.0),
                    tint: Color32::WHITE,
                }),
                local: Transform2D { position: vec2(0.0, 0.0), scale: vec2(1.0, 1.0), rotation: 0.0, pivot: vec2(64.0, 64.0) },
                z: 0.0, visible: true, alpha: 1.0, anim: PrimitiveAnimations::default(),
            };

            let t0 = 0.0f64;
            img_inst.anim.pos = Some(AnimTrack { start_time: t0, duration: 4.0, from: vec2(0.0, 0.0), to: vec2(300.0, 0.0), easing: Easing::EaseInOutCubic, repeat: RepeatMode::PingPong, enabled: true });
            img_inst.anim.rot = Some(AnimTrack { start_time: t0, duration: 5.0, from: 0.0, to: std::f32::consts::PI * 2.0, easing: Easing::Linear, repeat: RepeatMode::Loop, enabled: true });
            img_inst.anim.scale = Some(AnimTrack { start_time: t0, duration: 3.0, from: vec2(0.8, 0.8), to: vec2(1.2, 1.2), easing: Easing::EaseInOutQuad, repeat: RepeatMode::PingPong, enabled: true });
            img_inst.anim.opacity = Some(AnimTrack { start_time: t0, duration: 2.0, from: 0.4, to: 1.0, easing: Easing::EaseInOutQuad, repeat: RepeatMode::PingPong, enabled: true });

            node.primitives.push(img_inst);

            // Color rect under it
            let rect_inst = PrimitiveInstance {
                kind: PrimitiveKind::ColorRect(ColorRectPrimitive { size: vec2(300.0, 180.0), color: Color32::from_gray(40), corner_radius: 12.0 }),
                local: Transform2D { position: vec2(-40.0, -26.0), scale: vec2(1.0, 1.0), rotation: 0.1, pivot: vec2(0.0, 0.0) },
                z: -0.1, visible: true, alpha: 0.9, anim: PrimitiveAnimations::default(),
            };
            node.primitives.push(rect_inst);

            tree.root.children.push(node);
            tree.assign_ids_recursive();

            Self { renderer, tree, start: std::time::Instant::now() }
        }
    }

    impl eframe::App for DemoApp {
        fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
            let now = self.start.elapsed().as_secs_f64();
            let dt = ctx.input(|i| i.unstable_dt).max(1.0 / 120.0);

            // Paint to a central panel
            egui::CentralPanel::default().show(ctx, |ui| {
                let painter = ui.painter();
                // Update viewport in case window resized
                self.renderer.viewport = ui.max_rect();
                self.renderer.update_and_draw(painter, &self.tree, now, dt as f32);
            });

            ctx.request_repaint();
        }
    }

    pub fn run() -> eframe::Result<()> {
        let native_options = eframe::NativeOptions::default();
        eframe::run_native(
            "AVG RenderTree Demo",
            native_options,
            Box::new(|cc| Box::new(DemoApp::new(cc))),
        )
    }

    fn make_checker_image(w: usize, h: usize) -> ept::ColorImage {
        let mut img = ept::ColorImage::new([w, h], Color32::BLACK);
        for y in 0..h { for x in 0..w {
            let c = if ((x/16)+(y/16)) & 1 == 0 { Color32::LIGHT_BLUE } else { Color32::WHITE };
            img[(x, y)] = c;
        }}
        img
    }
}

