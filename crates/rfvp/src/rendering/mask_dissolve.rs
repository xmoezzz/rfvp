//! Shared 8-bit mask reveal. Source alpha is retained; only the GPU copy changes.
use crate::subsystem::resources::motion_manager::{DissolveType, MotionManager};
use alloc::{string::String, vec::Vec};
use image::{DynamicImage, GenericImageView, RgbaImage};

#[derive(Debug, Default)]
pub(crate) struct MaskDissolveFrame {
    source: Option<(u64, String)>,
    alpha: Vec<u8>,
    phase: u16,
    pub generation: u64,
    pub image: Option<DynamicImage>,
}

impl MaskDissolveFrame {
    pub fn prepare(&mut self, motion: &MotionManager) {
        let active = matches!(
            motion.get_dissolve_type(),
            DissolveType::MaskFadeIn | DissolveType::MaskFadeInOut | DissolveType::MaskFadeOut
        );
        let graph = motion.get_dissolve_mask_graph();
        let Some(texture) = graph.get_texture().as_ref().filter(|_| active) else {
            self.source = None;
            self.alpha = Vec::new();
            self.image = None;
            return;
        };
        let phase = (motion.get_dissolve_alpha().clamp(0.0, 1.0) * 510.0 + 0.5) as u16;
        let same = self.source.as_ref().map_or(false, |(generation, path)| {
            *generation == graph.generation && path == &graph.texture_path
        });
        if same && self.phase == phase {
            return;
        }
        if !same {
            let (w, h) = texture.dimensions();
            let Some((_, _, _, _, alpha)) = graph.export_alpha_mask() else {
                self.image = None;
                return;
            };
            self.alpha = alpha;
            self.image = Some(DynamicImage::ImageRgba8(RgbaImage::new(w, h)));
            self.source = Some((graph.generation, graph.texture_path.clone()));
        }
        let mut lut = [0u8; 256];
        for (alpha, value) in lut.iter_mut().enumerate() {
            *value = (alpha as i32 + i32::from(phase) - 255).clamp(0, 255) as u8;
        }
        let rgba = self.image.as_mut().unwrap().as_mut_rgba8().unwrap();
        for (pixel, alpha) in rgba.as_mut().chunks_exact_mut(4).zip(&self.alpha) {
            pixel[3] = lut[*alpha as usize];
        }
        self.phase = phase;
        self.generation = self.generation.wrapping_add(1);
    }
}
