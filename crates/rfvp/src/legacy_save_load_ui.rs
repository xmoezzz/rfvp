use anyhow::Result;
use fontdue::{Font, LineMetrics};
use glam::{Mat4, Vec2, Vec3, Vec4};
use image::{DynamicImage, Rgba, RgbaImage};
use wgpu::util::DeviceExt;
use winit::{event::{MouseButton, WindowEvent}, keyboard::{KeyCode, PhysicalKey}};

use crate::{
    rfvp_render::{GpuCommonResources, GpuTexture},
    rfvp_render::pipelines::sprite::SpritePipeline,
    rfvp_render::vertices::{PosColTexVertex, VertexSource},
    subsystem::{components::syscalls::legacy::LegacySaveLoadRequest, world::GameData},
};

const UI_PAGE_COUNT: usize = 8;
const UI_ROWS_PER_PAGE: usize = 25;
const UI_MAX_SLOT: usize = UI_PAGE_COUNT * UI_ROWS_PER_PAGE;
const DISSOLVE2_LOAD_COLOR_ID: u32 = 1;
const DISSOLVE2_LOAD_DURATION_MS: u32 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacySaveLoadMode {
    Load,
    Save,
    LoadTitle,
    SaveTitle,
}

impl From<LegacySaveLoadRequest> for LegacySaveLoadMode {
    fn from(value: LegacySaveLoadRequest) -> Self {
        match value {
            LegacySaveLoadRequest::LoadFile => Self::Load,
            LegacySaveLoadRequest::SaveFile => Self::Save,
            LegacySaveLoadRequest::LoadTitle => Self::LoadTitle,
            LegacySaveLoadRequest::SaveTitle => Self::SaveTitle,
        }
    }
}

pub struct LegacySaveLoadUi {
    active: bool,
    mode: LegacySaveLoadMode,
    page: usize,
    selected_row: usize,
    hover_row: Option<usize>,
    hover_cancel: bool,
    texture: GpuTexture,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    font: Font,
    dirty: bool,
}

fn format_slot_time(game: &GameData, slot: u32) -> String {
    let year = game.save_manager.get_year(slot);
    let month = game.save_manager.get_month(slot);
    let day = game.save_manager.get_day(slot);
    let hour = game.save_manager.get_hour(slot);
    let minute = game.save_manager.get_minute(slot);

    let valid =
        (2000..=9999).contains(&year) &&
        (1..=12).contains(&month) &&
        (1..=31).contains(&day) &&
        hour <= 23 &&
        minute <= 59;

    if !valid {
        return String::new();
    }

    format!(
        "{:04}/{:02}/{:02} {:02}:{:02}",
        year, month, day, hour, minute
    )
}

impl LegacySaveLoadUi {
    pub fn new(resources: &GpuCommonResources, virtual_size: (u32, u32)) -> Self {
        let (vw, vh) = virtual_size;
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(vw.max(1), vh.max(1), Rgba([0, 0, 0, 0])));
        let texture = GpuTexture::new(resources, &img, Some("legacy_save_load_ui"));

        let vertices = [
            PosColTexVertex { position: Vec3::new(0.0, 0.0, 0.0), color: Vec4::ONE, texture_coordinate: Vec2::new(0.0, 0.0) },
            PosColTexVertex { position: Vec3::new(vw as f32, 0.0, 0.0), color: Vec4::ONE, texture_coordinate: Vec2::new(1.0, 0.0) },
            PosColTexVertex { position: Vec3::new(vw as f32, vh as f32, 0.0), color: Vec4::ONE, texture_coordinate: Vec2::new(1.0, 1.0) },
            PosColTexVertex { position: Vec3::new(0.0, vh as f32, 0.0), color: Vec4::ONE, texture_coordinate: Vec2::new(0.0, 1.0) },
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let vertex_buffer = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("legacy_save_load_ui.vb"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("legacy_save_load_ui.ib"),
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
            mode: LegacySaveLoadMode::Load,
            page: 0,
            selected_row: 0,
            hover_row: None,
            hover_cancel: false,
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

    pub fn open(&mut self, request: LegacySaveLoadRequest, game: &mut GameData) {
        self.mode = request.into();
        self.active = true;
        self.hover_row = None;
        self.hover_cancel = false;
        let cur_slot = game.save_manager.get_current_save_slot();
        let slot = if (cur_slot as usize) < UI_MAX_SLOT { cur_slot as usize } else { 0 };
        self.page = slot / UI_ROWS_PER_PAGE;
        self.selected_row = slot % UI_ROWS_PER_PAGE;
        let _ = game.save_manager.refresh_all_savedata(game.get_nls());
        self.dirty = true;
    }

    pub fn close(&mut self) {
        self.active = false;
        self.hover_row = None;
        self.hover_cancel = false;
    }

    pub fn handle_window_event(
        &mut self,
        event: &WindowEvent,
        surface_size: (u32, u32),
        virtual_size: (u32, u32),
        game: &mut GameData,
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
                        self.close();
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) | PhysicalKey::Code(KeyCode::Space) => {
                        self.confirm(game);
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::ArrowUp) => self.move_row(-1),
                    PhysicalKey::Code(KeyCode::ArrowDown) => self.move_row(1),
                    PhysicalKey::Code(KeyCode::ArrowLeft) | PhysicalKey::Code(KeyCode::PageUp) => self.move_page(-1),
                    PhysicalKey::Code(KeyCode::ArrowRight) | PhysicalKey::Code(KeyCode::PageDown) => self.move_page(1),
                    PhysicalKey::Code(KeyCode::Home) => {
                        self.selected_row = 0;
                        self.dirty = true;
                    }
                    PhysicalKey::Code(KeyCode::End) => {
                        self.selected_row = UI_ROWS_PER_PAGE.saturating_sub(1);
                        self.dirty = true;
                    }
                    _ => {}
                }
                return true;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some((vx, vy)) = map_window_to_virtual(position.x, position.y, surface_size, virtual_size) {
                    let hover = self.hit_test_row(vx, vy, virtual_size);
                    let hover_cancel = Self::hit_test_cancel_button(vx, vy, virtual_size);
                    if hover != self.hover_row || hover_cancel != self.hover_cancel {
                        self.hover_row = hover;
                        self.hover_cancel = hover_cancel;
                        self.dirty = true;
                    }
                } else if self.hover_row.is_some() || self.hover_cancel {
                    self.hover_row = None;
                    self.hover_cancel = false;
                    self.dirty = true;
                }
                return true;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if *button != MouseButton::Left || !state.is_pressed() {
                    return true;
                }
                if self.hover_cancel {
                    self.close();
                    return true;
                }
                if let Some(row) = self.hover_row {
                    self.selected_row = row;
                    self.dirty = true;
                    return true;
                }
                return true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => {
                        if *y > 0.0 {
                            self.move_row(-1);
                        } else if *y < 0.0 {
                            self.move_row(1);
                        }
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        if pos.y > 0.0 {
                            self.move_row(-1);
                        } else if pos.y < 0.0 {
                            self.move_row(1);
                        }
                    }
                }
                return true;
            }
            WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => {
                self.hover_row = None;
                self.hover_cancel = false;
                self.dirty = true;
                return true;
            }
            _ => {}
        }

        false
    }

    pub fn handle_touch(
        &mut self,
        phase: i32,
        x_px: f64,
        y_px: f64,
        surface_size: (u32, u32),
        virtual_size: (u32, u32),
        game: &mut GameData,
    ) -> bool {
        if !self.active {
            return false;
        }

        let Some((vx, vy)) = map_window_to_virtual(x_px, y_px, surface_size, virtual_size) else {
            self.hover_row = None;
            self.hover_cancel = false;
            self.dirty = true;
            return true;
        };

        let hover = self.hit_test_row(vx, vy, virtual_size);
        let hover_cancel = Self::hit_test_cancel_button(vx, vy, virtual_size);
        if hover != self.hover_row || hover_cancel != self.hover_cancel {
            self.hover_row = hover;
            self.hover_cancel = hover_cancel;
            self.dirty = true;
        }

        match phase {
            0 => {
                if hover_cancel {
                    self.close();
                } else if let Some(row) = hover {
                    self.selected_row = row;
                    self.dirty = true;
                }
            }
            2 | 3 => {
                if hover_cancel {
                    self.close();
                }
            }
            _ => {}
        }

        let _ = game;
        true
    }

    pub fn update(&mut self, resources: &GpuCommonResources, game: &mut GameData, virtual_size: (u32, u32)) {
        if !self.active || !self.dirty {
            return;
        }
        let image = self.build_image(game, virtual_size);
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

    fn move_row(&mut self, delta: isize) {
        let max = UI_ROWS_PER_PAGE.saturating_sub(1) as isize;
        let next = (self.selected_row as isize + delta).clamp(0, max) as usize;
        if next != self.selected_row {
            self.selected_row = next;
            self.dirty = true;
        }
    }

    fn move_page(&mut self, delta: isize) {
        let max = UI_PAGE_COUNT.saturating_sub(1) as isize;
        let next = (self.page as isize + delta).clamp(0, max) as usize;
        if next != self.page {
            self.page = next;
            self.dirty = true;
        }
    }

    fn confirm(&mut self, game: &mut GameData) {
        let slot = (self.page * UI_ROWS_PER_PAGE + self.selected_row) as u32;
        match self.mode {
            LegacySaveLoadMode::Load | LegacySaveLoadMode::LoadTitle => {
                if !game.save_manager.test_save_slot(slot) {
                    return;
                }
                game.save_manager.request_load(slot);
                game.motion_manager.start_dissolve2_in_out(DISSOLVE2_LOAD_COLOR_ID, DISSOLVE2_LOAD_DURATION_MS);
                self.close();
            }
            LegacySaveLoadMode::Save | LegacySaveLoadMode::SaveTitle => {
                if !game.save_manager.has_local_saved() {
                    return;
                }
                game.save_manager.set_current_save_slot(slot);
                game.save_manager.set_savedata_requested(true);
                self.close();
            }
        }
    }

    fn mode_label(&self) -> &'static str {
        match self.mode {
            LegacySaveLoadMode::Load => "LOAD",
            LegacySaveLoadMode::Save => "SAVE",
            LegacySaveLoadMode::LoadTitle => "LOAD(TITLE)",
            LegacySaveLoadMode::SaveTitle => "SAVE(TITLE)",
        }
    }

    fn build_image(&self, game: &mut GameData, virtual_size: (u32, u32)) -> RgbaImage {
        let (vw, vh) = virtual_size;
        let mut img = RgbaImage::from_pixel(vw.max(1), vh.max(1), Rgba([0, 0, 0, 160]));
        let panel_x = 24i32;
        let panel_y = 18i32;
        let panel_w = vw as i32 - 48;
        let panel_h = vh as i32 - 36;
        fill_rect(&mut img, panel_x, panel_y, panel_w, panel_h, [238, 238, 238, 245]);
        stroke_rect(&mut img, panel_x, panel_y, panel_w, panel_h, [24, 24, 24, 255]);
        fill_rect(&mut img, panel_x + 1, panel_y + 1, panel_w - 2, 28, [48, 72, 112, 255]);
        draw_text_left(&self.font, &mut img, (panel_x + 10, panel_y + 4, panel_w - 20, 20), 16.0, [255, 255, 255, 255], self.mode_label());

        let tabs_y = panel_y + 34;
        let tabs_x = panel_x + 8;
        let tabs_w = panel_w - 16;
        let tab_w = (tabs_w / UI_PAGE_COUNT as i32).max(48);
        for page in 0..UI_PAGE_COUNT {
            let x = tabs_x + page as i32 * tab_w;
            let active = page == self.page;
            fill_rect(&mut img, x, tabs_y, tab_w - 2, 22, if active { [255, 255, 255, 255] } else { [188, 196, 212, 255] });
            stroke_rect(&mut img, x, tabs_y, tab_w - 2, 22, [24, 24, 24, 255]);
            let label = format!("PAGE{}", page + 1);
            draw_text_centered(&self.font, &mut img, (x + 2, tabs_y + 2, tab_w - 6, 18), 12.0, [0, 0, 0, 255], &label);
        }

        let list_x = panel_x + 8;
        let list_y = tabs_y + 30;
        let list_w = panel_w - 16;
        let footer_h = 34;
        let list_h = panel_h - (list_y - panel_y) - footer_h - 8;
        fill_rect(&mut img, list_x, list_y, list_w, list_h, [248, 248, 248, 255]);
        stroke_rect(&mut img, list_x, list_y, list_w, list_h, [32, 32, 32, 255]);

        let c0 = (list_w as f32 * 0.42) as i32;
        let c1 = (list_w as f32 * 0.28) as i32;
        let c2 = list_w - c0 - c1;
        fill_rect(&mut img, list_x + c0, list_y, 1, list_h, [96, 96, 96, 255]);
        fill_rect(&mut img, list_x + c0 + c1, list_y, 1, list_h, [96, 96, 96, 255]);

        let header_h = 18;
        fill_rect(&mut img, list_x, list_y, list_w, header_h, [216, 220, 228, 255]);
        draw_text_left(&self.font, &mut img, (list_x + 4, list_y + 1, c0 - 8, header_h - 2), 12.0, [0, 0, 0, 255], "TITLE");
        draw_text_left(&self.font, &mut img, (list_x + c0 + 4, list_y + 1, c1 - 8, header_h - 2), 12.0, [0, 0, 0, 255], "SCENE");
        draw_text_left(&self.font, &mut img, (list_x + c0 + c1 + 4, list_y + 1, c2 - 8, header_h - 2), 12.0, [0, 0, 0, 255], "TIME");

        let rows_top = list_y + header_h;
        let row_h = ((list_h - header_h) / UI_ROWS_PER_PAGE as i32).max(12);
        for row in 0..UI_ROWS_PER_PAGE {
            let slot = self.page * UI_ROWS_PER_PAGE + row;
            let y = rows_top + row as i32 * row_h;
            let selected = row == self.selected_row;
            let hovered = self.hover_row == Some(row);
            let bg = if selected {
                [132, 164, 224, 255]
            } else if hovered {
                [224, 232, 244, 255]
            } else if row % 2 == 0 {
                [248, 248, 248, 255]
            } else {
                [240, 240, 240, 255]
            };
            fill_rect(&mut img, list_x + 1, y, list_w - 2, row_h, bg);
            fill_rect(&mut img, list_x, y + row_h - 1, list_w, 1, [220, 220, 220, 255]);

            let exists = slot < UI_MAX_SLOT && game.save_manager.test_save_slot(slot as u32);
            let title = if exists {
                let t = game.save_manager.get_save_title(slot as u32);
                format!("{:03}. {}", slot + 1, if t.trim().is_empty() { "<EMPTY>" } else { &t })
            } else {
                format!("{:03}. <EMPTY>", slot + 1)
            };
            let scene = if exists { game.save_manager.get_save_scene_title(slot as u32) } else { String::new() };
            let time = if exists {
                format_slot_time(game, slot as u32)
            } else {
                String::new()
            };
            let color = if exists { [0, 0, 0, 255] } else { [96, 96, 96, 255] };
            draw_text_left(&self.font, &mut img, (list_x + 4, y + 1, c0 - 8, row_h - 2), 11.0, color, &title);
            draw_text_left(&self.font, &mut img, (list_x + c0 + 4, y + 1, c1 - 8, row_h - 2), 11.0, color, &scene);
            draw_text_left(&self.font, &mut img, (list_x + c0 + c1 + 4, y + 1, c2 - 8, row_h - 2), 11.0, color, &time);
        }

        let footer_y = panel_y + panel_h - footer_h;
        let msg = match self.mode {
            LegacySaveLoadMode::Load | LegacySaveLoadMode::LoadTitle => "Enter: load selected slot, Esc: cancel",
            LegacySaveLoadMode::Save | LegacySaveLoadMode::SaveTitle => {
                if game.save_manager.has_local_saved() {
                    "Enter: save to selected slot, Esc: cancel"
                } else {
                    "Preparing save data..."
                }
            }
        };
        let (btn_x, btn_y, btn_w, btn_h) = Self::cancel_button_rect(virtual_size);
        let btn_bg = if self.hover_cancel { [184, 196, 220, 255] } else { [216, 220, 228, 255] };
        fill_rect(&mut img, btn_x, btn_y, btn_w, btn_h, btn_bg);
        stroke_rect(&mut img, btn_x, btn_y, btn_w, btn_h, [32, 32, 32, 255]);
        draw_text_centered(&self.font, &mut img, (btn_x + 2, btn_y + 2, btn_w - 4, btn_h - 4), 12.0, [0, 0, 0, 255], "ESC");

        draw_text_left(&self.font, &mut img, (panel_x + 10, footer_y + 6, (btn_x - (panel_x + 10) - 8).max(0), 18), 12.0, [0, 0, 0, 255], msg);

        img
    }

    fn cancel_button_rect(virtual_size: (u32, u32)) -> (i32, i32, i32, i32) {
        let (vw, vh) = virtual_size;
        let panel_x = 24i32;
        let panel_y = 18i32;
        let panel_w = vw as i32 - 48;
        let panel_h = vh as i32 - 36;
        let footer_h = 34;
        let footer_y = panel_y + panel_h - footer_h;
        let btn_w = 60i32;
        let btn_h = 22i32;
        let btn_x = panel_x + panel_w - btn_w - 10;
        let btn_y = footer_y + 4;
        (btn_x, btn_y, btn_w, btn_h)
    }

    fn hit_test_cancel_button(x: i32, y: i32, virtual_size: (u32, u32)) -> bool {
        let (bx, by, bw, bh) = Self::cancel_button_rect(virtual_size);
        x >= bx && x < bx + bw && y >= by && y < by + bh
    }

    fn hit_test_row(&self, x: i32, y: i32, virtual_size: (u32, u32)) -> Option<usize> {
        let (vw, vh) = virtual_size;
        let panel_x = 24i32;
        let panel_y = 18i32;
        let panel_w = vw as i32 - 48;
        let panel_h = vh as i32 - 36;
        let list_x = panel_x + 8;
        let tabs_y = panel_y + 34;
        let list_y = tabs_y + 30;
        let list_w = panel_w - 16;
        let footer_h = 34;
        let list_h = panel_h - (list_y - panel_y) - footer_h - 8;
        let header_h = 18;
        let rows_top = list_y + header_h;
        let row_h = ((list_h - header_h) / UI_ROWS_PER_PAGE as i32).max(12);
        if x < list_x || x >= list_x + list_w || y < rows_top || y >= rows_top + row_h * UI_ROWS_PER_PAGE as i32 {
            return None;
        }
        let row = ((y - rows_top) / row_h) as usize;
        if row < UI_ROWS_PER_PAGE { Some(row) } else { None }
    }
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
    if w <= 0 || h <= 0 { return; }
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
    if w <= 0 || h <= 0 { return; }
    for yy in 0..h {
        for xx in 0..w {
            let idx = (yy * w + xx) as usize;
            let cov = bitmap.get(idx).copied().unwrap_or(0) as u32;
            if cov == 0 { continue; }
            let px = x + xx;
            let py = y + yy;
            if px < 0 || py < 0 || px >= img.width() as i32 || py >= img.height() as i32 { continue; }
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
