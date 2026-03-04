use std::sync::Arc;

use crate::audio_player::bgm_player::BgmDebugSummary;
use crate::audio_player::se_player::SeDebugSummary;
use crate::debug_ui::log_ring::LogRing;
use crate::debug_ui::vm_snapshot::VmSnapshot;
use crate::rendering::gpu_prim::{DebugPrimTile, GpuPrimRenderer, PrimRenderStats};

#[derive(Debug, Clone, Copy, Default)]
pub struct HudInput {
    /// Pointer position in points (logical pixels). None means "pointer not in window".
    pub pointer_pos: Option<(f32, f32)>,
    pub pointer_down: bool,
    pub scroll_delta_y: f32,
}

#[derive(Debug, Clone, Default)]
pub struct HudSnapshot {
    pub frame_no: u64,
    pub dt_ms: f32,
    /// Human-readable input summary line (keys/mouse state).
    pub input_line: String,
    pub render: PrimRenderStats,
    pub se: SeDebugSummary,
    pub bgm: BgmDebugSummary,
    pub vm: VmSnapshot,
    pub textures: Vec<String>,
    pub text_slots: Vec<String>,
    pub text_lines: Vec<String>,
    pub prim_tiles: Vec<DebugPrimTile>,
}

/// Stub implementation for non-desktop targets.
///
/// This keeps the engine code (which unconditionally references HUD types) compiling,
/// while avoiding the desktop-only egui-wgpu renderer stack.
pub struct DebugHud {
    _ring: Arc<LogRing>,
}

impl DebugHud {
    pub fn new(_device: &wgpu::Device, _surface_format: wgpu::TextureFormat, ring: Arc<LogRing>) -> Self {
        Self { _ring: ring }
    }

    pub fn set_max_console_lines(&mut self, _n: usize) {}

    pub fn sync_prim_tile_textures(
        &mut self,
        _device: &wgpu::Device,
        _prim_renderer: &GpuPrimRenderer,
        _tiles: &[DebugPrimTile],
    ) {
    }

    pub fn prepare_frame(
        &mut self,
        _window_size_px: (u32, u32),
        _pixels_per_point: f32,
        _snap: &HudSnapshot,
        _input: Option<HudInput>,
    ) {
    }

    pub fn render(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _encoder: &mut wgpu::CommandEncoder,
        _view: &wgpu::TextureView,
    ) {
    }
}
