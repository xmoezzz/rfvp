use fontdue::{Font, LineMetrics};
use glam::{Mat4, Vec2, Vec3, Vec4};
use image::{DynamicImage, Rgba, RgbaImage};
use wgpu::util::DeviceExt;
use winit::{
    event::{MouseButton, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

use crate::subsystem::world::GameData;

use crate::rfvp_render::pipelines::sprite::SpritePipeline;
use crate::rfvp_render::vertices::{PosColTexVertex, VertexSource};
use crate::rfvp_render::{GpuCommonResources, GpuTexture};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitConfirmOutcome {
    Confirmed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExitButton {
    Yes,
    No,
}

pub struct ExitConfirmUi {
    active: bool,
    selected: ExitButton,
    hover: Option<ExitButton>,
    pressed: Option<ExitButton>,
    outcome: Option<ExitConfirmOutcome>,
    texture: GpuTexture,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    font: Font,
    dirty: bool,
}

impl ExitConfirmUi {
    pub fn new(resources: &GpuCommonResources, virtual_size: (u32, u32)) -> Self {
        let (vw, vh) = virtual_size;
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(vw.max(1), vh.max(1), Rgba([0, 0, 0, 0])));
        let texture = GpuTexture::new(resources, &img, Some("exit_confirm_ui"));

        let vertices = [
            PosColTexVertex { position: Vec3::new(0.0, 0.0, 0.0), color: Vec4::ONE, texture_coordinate: Vec2::new(0.0, 0.0) },
            PosColTexVertex { position: Vec3::new(vw as f32, 0.0, 0.0), color: Vec4::ONE, texture_coordinate: Vec2::new(1.0, 0.0) },
            PosColTexVertex { position: Vec3::new(vw as f32, vh as f32, 0.0), color: Vec4::ONE, texture_coordinate: Vec2::new(1.0, 1.0) },
            PosColTexVertex { position: Vec3::new(0.0, vh as f32, 0.0), color: Vec4::ONE, texture_coordinate: Vec2::new(0.0, 1.0) },
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let vertex_buffer = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exit_confirm_ui.vb"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exit_confirm_ui.ib"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let font = Font::from_bytes(
            include_bytes!("subsystem/resources/fonts/MSGOTHIC.TTF") as &[u8],
            fontdue::FontSettings::default(),
        )
        .expect("embedded MSGOTHIC.TTF must be valid");

        Self {
            active: false,
            selected: ExitButton::No,
            hover: None,
            pressed: None,
            outcome: None,
            texture,
            vertex_buffer,
            index_buffer,
            font,
            dirty: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self) {
        self.active = true;
        self.selected = ExitButton::No;
        self.hover = None;
        self.pressed = None;
        self.outcome = None;
        self.dirty = true;
    }

    pub fn close(&mut self) {
        self.active = false;
        self.hover = None;
        self.pressed = None;
        self.dirty = true;
    }

    pub fn take_outcome(&mut self) -> Option<ExitConfirmOutcome> {
        self.outcome.take()
    }

    pub fn handle_window_event(
        &mut self,
        event: &WindowEvent,
        surface_size: (u32, u32),
        virtual_size: (u32, u32),
    ) -> bool {
        if !self.active {
            return false;
        }

        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if !event.state.is_pressed() || event.repeat {
                    return true;
                }
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.cancel();
                    }
                    PhysicalKey::Code(KeyCode::Enter)
                    | PhysicalKey::Code(KeyCode::NumpadEnter)
                    | PhysicalKey::Code(KeyCode::Space) => {
                        self.confirm_selected();
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft)
                    | PhysicalKey::Code(KeyCode::ArrowUp)
                    | PhysicalKey::Code(KeyCode::Tab) => {
                        self.toggle_selection();
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight)
                    | PhysicalKey::Code(KeyCode::ArrowDown) => {
                        self.toggle_selection();
                    }
                    _ => {}
                }
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.hover = map_window_to_virtual(position.x, position.y, surface_size, virtual_size)
                    .and_then(|(vx, vy)| self.hit_test_button(vx, vy, virtual_size));
                self.dirty = true;
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if *button != MouseButton::Left {
                    return true;
                }
                if state.is_pressed() {
                    self.pressed = self.hover;
                    if let Some(btn) = self.hover {
                        self.selected = btn;
                        self.dirty = true;
                    }
                } else {
                    let pressed = self.pressed.take();
                    if let (Some(pressed), Some(hover)) = (pressed, self.hover) {
                        if pressed == hover {
                            self.selected = hover;
                            match hover {
                                ExitButton::Yes => self.confirm_yes(),
                                ExitButton::No => self.cancel(),
                            }
                        }
                    }
                }
                true
            }
            WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => {
                self.hover = None;
                self.pressed = None;
                self.dirty = true;
                true
            }
            _ => true,
        }
    }

    pub fn handle_touch(
        &mut self,
        phase: i32,
        x_px: f64,
        y_px: f64,
        surface_size: (u32, u32),
        virtual_size: (u32, u32),
    ) -> bool {
        if !self.active {
            return false;
        }

        let hit = map_window_to_virtual(x_px, y_px, surface_size, virtual_size)
            .and_then(|(vx, vy)| self.hit_test_button(vx, vy, virtual_size));

        match phase {
            0 => {
                self.pressed = hit;
                self.hover = hit;
                if let Some(btn) = hit {
                    self.selected = btn;
                }
                self.dirty = true;
            }
            1 => {
                self.hover = hit;
                self.dirty = true;
            }
            2 => {
                let pressed = self.pressed.take();
                self.hover = hit;
                if let (Some(pressed), Some(hit)) = (pressed, hit) {
                    if pressed == hit {
                        self.selected = hit;
                        match hit {
                            ExitButton::Yes => self.confirm_yes(),
                            ExitButton::No => self.cancel(),
                        }
                    }
                } else {
                    self.dirty = true;
                }
            }
            3 => {
                self.pressed = None;
                self.hover = None;
                self.dirty = true;
            }
            _ => {}
        }
        true
    }

    pub fn update(&mut self, resources: &GpuCommonResources, virtual_size: (u32, u32)) {
        if !self.active || !self.dirty {
            return;
        }
        let image = self.build_image(virtual_size);
        let dyn_img = DynamicImage::ImageRgba8(image);
        let _ = self.texture.update_rgba8(resources, &dyn_img);
        self.dirty = false;
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        sprite: &'a SpritePipeline,
        projection_matrix: Mat4,
    ) {
        if !self.active {
            return;
        }
        let src = VertexSource::VertexIndexBuffer {
            vertex_buffer: &self.vertex_buffer,
            index_buffer: &self.index_buffer,
            indices: 0..6,
            instances: 0..1,
        };
        sprite.draw(pass, src, self.texture.bind_group(), projection_matrix);
    }

    fn toggle_selection(&mut self) {
        self.selected = match self.selected {
            ExitButton::Yes => ExitButton::No,
            ExitButton::No => ExitButton::Yes,
        };
        self.dirty = true;
    }

    fn confirm_selected(&mut self) {
        match self.selected {
            ExitButton::Yes => self.confirm_yes(),
            ExitButton::No => self.cancel(),
        }
    }

    fn confirm_yes(&mut self) {
        self.outcome = Some(ExitConfirmOutcome::Confirmed);
        self.close();
    }

    fn cancel(&mut self) {
        self.outcome = Some(ExitConfirmOutcome::Cancelled);
        self.close();
    }

    fn build_image(&self, virtual_size: (u32, u32)) -> RgbaImage {
        let (vw, vh) = virtual_size;
        let vw = vw.max(1) as i32;
        let vh = vh.max(1) as i32;
        let mut img = RgbaImage::from_pixel(vw as u32, vh as u32, Rgba([0, 0, 0, 160]));

        let panel_w = (vw * 3 / 5).clamp(280, vw - 32);
        let panel_h = (vh / 3).clamp(120, vh - 32);
        let panel_x = (vw - panel_w) / 2;
        let panel_y = (vh - panel_h) / 2;

        fill_rect(&mut img, panel_x, panel_y, panel_w, panel_h, [238, 238, 238, 246]);
        stroke_rect(&mut img, panel_x, panel_y, panel_w, panel_h, [24, 24, 24, 255]);
        fill_rect(&mut img, panel_x + 1, panel_y + 1, panel_w - 2, 28, [72, 88, 128, 255]);
        draw_text_left(&self.font, &mut img, (panel_x + 10, panel_y + 4, panel_w - 20, 20), 16.0, [255, 255, 255, 255], "EXIT");

        draw_text_left(
            &self.font,
            &mut img,
            (panel_x + 16, panel_y + 40, panel_w - 32, 24),
            16.0,
            [0, 0, 0, 255],
            "Are you sure you want to quit?",
        );
        draw_text_left(
            &self.font,
            &mut img,
            (panel_x + 16, panel_y + 68, panel_w - 32, 18),
            12.0,
            [48, 48, 48, 255],
            "Enter: confirm  Esc: cancel",
        );

        let button_w = 96;
        let button_h = 28;
        let gap = 20;
        let total_w = button_w * 2 + gap;
        let base_x = panel_x + (panel_w - total_w) / 2;
        let button_y = panel_y + panel_h - 44;

        for (idx, btn, label) in [
            (0, ExitButton::Yes, "YES"),
            (1, ExitButton::No, "NO"),
        ] {
            let x = base_x + idx * (button_w + gap);
            let is_selected = self.selected == btn;
            let is_hover = self.hover == Some(btn);
            let fill = if is_selected {
                [132, 164, 224, 255]
            } else if is_hover {
                [224, 232, 244, 255]
            } else {
                [248, 248, 248, 255]
            };
            fill_rect(&mut img, x, button_y, button_w, button_h, fill);
            stroke_rect(&mut img, x, button_y, button_w, button_h, [24, 24, 24, 255]);
            draw_text_centered(&self.font, &mut img, (x + 6, button_y + 3, button_w - 12, button_h - 6), 13.0, [0, 0, 0, 255], label);
        }

        img
    }

    fn hit_test_button(&self, x: i32, y: i32, virtual_size: (u32, u32)) -> Option<ExitButton> {
        let (vw, vh) = virtual_size;
        let vw = vw.max(1) as i32;
        let vh = vh.max(1) as i32;
        let panel_w = (vw * 3 / 5).clamp(280, vw - 32);
        let panel_h = (vh / 3).clamp(120, vh - 32);
        let panel_x = (vw - panel_w) / 2;
        let panel_y = (vh - panel_h) / 2;
        let button_w = 96;
        let button_h = 28;
        let gap = 20;
        let total_w = button_w * 2 + gap;
        let base_x = panel_x + (panel_w - total_w) / 2;
        let button_y = panel_y + panel_h - 44;
        let yes = (base_x, button_y, button_w, button_h);
        let no = (base_x + button_w + gap, button_y, button_w, button_h);
        if inside_rect(x, y, yes) {
            Some(ExitButton::Yes)
        } else if inside_rect(x, y, no) {
            Some(ExitButton::No)
        } else {
            None
        }
    }
}

fn inside_rect(x: i32, y: i32, rect: (i32, i32, i32, i32)) -> bool {
    let (rx, ry, rw, rh) = rect;
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

fn map_window_to_virtual(px: f64, py: f64, surface_size: (u32, u32), virtual_size: (u32, u32)) -> Option<(i32, i32)> {
    let (sw_u, sh_u) = surface_size;
    let (vw_u, vh_u) = virtual_size;
    let sw = sw_u.max(1) as f64;
    let sh = sh_u.max(1) as f64;
    let vw = vw_u.max(1) as f64;
    let vh = vh_u.max(1) as f64;
    let scale = (sw / vw).min(sh / vh);
    let dst_w = vw * scale;
    let dst_h = vh * scale;
    let off_x = (sw - dst_w) * 0.5;
    let off_y = (sh - dst_h) * 0.5;
    if px < off_x || px >= off_x + dst_w || py < off_y || py >= off_y + dst_h {
        return None;
    }
    let vx = ((px - off_x) / scale) as i32;
    let vy = ((py - off_y) / scale) as i32;
    Some((vx.clamp(0, vw as i32 - 1), vy.clamp(0, vh as i32 - 1)))
}

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
    if w <= 0 || h <= 0 {
        return;
    }
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let x1 = (x + w).min(img.width() as i32).max(0) as u32;
    let y1 = (y + h).min(img.height() as i32).max(0) as u32;
    for yy in y0..y1 {
        for xx in x0..x1 {
            img.put_pixel(xx, yy, Rgba(color));
        }
    }
}

fn stroke_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
    fill_rect(img, x, y, w, 1, color);
    fill_rect(img, x, y + h - 1, w, 1, color);
    fill_rect(img, x, y, 1, h, color);
    fill_rect(img, x + w - 1, y, 1, h, color);
}

fn line_metrics(font: &Font, size: f32) -> LineMetrics {
    font.horizontal_line_metrics(size).unwrap_or(LineMetrics {
        ascent: size * 0.8,
        descent: -size * 0.2,
        line_gap: 0.0,
        new_line_size: size,
    })
}

#[derive(Clone, Copy)]
struct GlyphLayout {
    xmin: i32,
    ymin: i32,
    width: i32,
    height: i32,
    advance: i32,
    bitmap_offset: usize,
    bitmap_len: usize,
}

struct TextLayout {
    glyphs: Vec<GlyphLayout>,
    bitmap: Vec<u8>,
    width: i32,
    min_y: i32,
    max_y: i32,
}

impl TextLayout {
    fn height(&self) -> i32 {
        (self.max_y - self.min_y).max(0)
    }
}

fn layout_text(font: &Font, size: f32, text: &str, max_width: i32) -> TextLayout {
    let mut glyphs = Vec::new();
    let mut bitmap = Vec::new();
    let mut pen_x = 0i32;
    let mut min_y = 0i32;
    let mut max_y = 0i32;
    let mut have_bbox = false;
    let mut width = 0i32;

    for ch in text.chars() {
        let metrics = font.metrics(ch, size);
        let advance = metrics.advance_width.ceil() as i32;
        let next_pen_x = pen_x + advance;
        if next_pen_x > max_width {
            break;
        }

        let (rmetrics, rbitmap) = font.rasterize(ch, size);
        if rmetrics.width > 0 && rmetrics.height > 0 {
            let ymin = rmetrics.ymin;
            let ymax = rmetrics.ymin + rmetrics.height as i32;
            if !have_bbox {
                min_y = ymin;
                max_y = ymax;
                have_bbox = true;
            } else {
                min_y = min_y.min(ymin);
                max_y = max_y.max(ymax);
            }
        }

        let bitmap_offset = bitmap.len();
        bitmap.extend_from_slice(&rbitmap);
        let bitmap_len = rbitmap.len();
        glyphs.push(GlyphLayout {
            xmin: rmetrics.xmin,
            ymin: rmetrics.ymin,
            width: rmetrics.width as i32,
            height: rmetrics.height as i32,
            advance,
            bitmap_offset,
            bitmap_len,
        });
        pen_x = next_pen_x;
        width = pen_x;
    }

    if !have_bbox {
        let lm = line_metrics(font, size);
        min_y = 0;
        max_y = (lm.new_line_size.ceil() as i32).max(0);
    }

    TextLayout { glyphs, bitmap, width, min_y, max_y }
}

fn draw_text_left(font: &Font, img: &mut RgbaImage, rect: (i32, i32, i32, i32), size: f32, color: [u8; 4], text: &str) {
    let (x, y, w, h) = rect;
    if w <= 0 || h <= 0 {
        return;
    }
    let layout = layout_text(font, size, text, w.max(0));
    let lm = line_metrics(font, size);
    let line_h = (lm.ascent - lm.descent).ceil() as i32;
    let baseline = y + ((h - line_h).max(0) / 2) + lm.ascent.ceil() as i32;
    let mut pen_x = x;
    for glyph in &layout.glyphs {
        let gx = pen_x + glyph.xmin;
        let gy = baseline - (glyph.ymin + glyph.height);
        draw_glyph(
            img,
            gx,
            gy,
            glyph.width,
            glyph.height,
            &layout.bitmap[glyph.bitmap_offset..glyph.bitmap_offset + glyph.bitmap_len],
            color,
        );
        pen_x += glyph.advance;
    }
}

fn draw_text_centered(font: &Font, img: &mut RgbaImage, rect: (i32, i32, i32, i32), size: f32, color: [u8; 4], text: &str) {
    let (x, y, w, h) = rect;
    if w <= 0 || h <= 0 {
        return;
    }
    let layout = layout_text(font, size, text, w.max(0));
    let tx = x + ((w - layout.width).max(0) / 2);
    let lm = line_metrics(font, size);
    let line_h = (lm.ascent - lm.descent).ceil() as i32;
    let baseline = y + ((h - line_h).max(0) / 2) + lm.ascent.ceil() as i32;
    let mut pen_x = tx;
    for glyph in &layout.glyphs {
        let gx = pen_x + glyph.xmin;
        let gy = baseline - (glyph.ymin + glyph.height);
        draw_glyph(
            img,
            gx,
            gy,
            glyph.width,
            glyph.height,
            &layout.bitmap[glyph.bitmap_offset..glyph.bitmap_offset + glyph.bitmap_len],
            color,
        );
        pen_x += glyph.advance;
    }
}

fn draw_glyph(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, bitmap: &[u8], color: [u8; 4]) {
    if w <= 0 || h <= 0 {
        return;
    }
    for yy in 0..h {
        for xx in 0..w {
            let idx = (yy * w + xx) as usize;
            let cov = bitmap.get(idx).copied().unwrap_or(0) as u32;
            if cov == 0 {
                continue;
            }
            let px = x + xx;
            let py = y + yy;
            if px < 0 || py < 0 || px >= img.width() as i32 || py >= img.height() as i32 {
                continue;
            }
            let dst = img.get_pixel_mut(px as u32, py as u32);
            let src_a = (color[3] as u32 * cov) / 255;
            let inv_a = 255u32.saturating_sub(src_a);
            for c in 0..3 {
                dst.0[c] = (((color[c] as u32 * src_a) + (dst.0[c] as u32 * inv_a)) / 255) as u8;
            }
            dst.0[3] = ((src_a + (dst.0[3] as u32 * inv_a) / 255).min(255)) as u8;
        }
    }
}
