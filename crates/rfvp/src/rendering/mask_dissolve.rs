//! Shared 8-bit mask dissolve. Source alpha is retained; only the GPU copy changes.
//!
//! A mask dissolve (`Dissolve` with a mask, wait=4/5) wipes the frame shown when it started away
//! through the mask, revealing the scene under it; the renderer hands that frame over with
//! `set_snapshot`. Only `MaskFadeOut` still fades the mask to black.
use crate::subsystem::resources::motion_manager::{DissolveType, MotionManager};
use alloc::{string::String, vec::Vec};
use image::{DynamicImage, GenericImageView, RgbaImage};

#[derive(Debug, Default)]
pub(crate) struct MaskDissolveFrame {
    /// Mask generation and path, and the serial of the snapshot the image was built from.
    source: Option<(u64, String, Option<u32>)>,
    alpha: Vec<u8>,
    phase: u16,
    snapshot: Option<(u32, RgbaImage)>,
    pub generation: u64,
    pub image: Option<DynamicImage>,
}

fn wipes(motion: &MotionManager) -> bool {
    matches!(motion.get_dissolve_type(), DissolveType::MaskFadeIn | DissolveType::MaskFadeInOut)
}

impl MaskDissolveFrame {
    /// Whether the renderer must capture the frame it last showed (before drawing the next one).
    pub fn wants_snapshot(&self, motion: &MotionManager) -> bool {
        wipes(motion)
            && self.snapshot.as_ref().map_or(true, |(serial, _)| *serial != motion.get_dissolve_serial())
    }

    /// The last frame shown, in the render target's own color encoding.
    pub fn set_snapshot(&mut self, motion: &MotionManager, frame: RgbaImage) {
        self.snapshot = Some((motion.get_dissolve_serial(), frame));
    }

    pub fn prepare(&mut self, motion: &MotionManager) {
        let active = matches!(
            motion.get_dissolve_type(),
            DissolveType::MaskFadeIn | DissolveType::MaskFadeInOut | DissolveType::MaskFadeOut
        );
        let graph = motion.get_dissolve_mask_graph();
        let texture = graph.get_texture().as_ref().filter(|_| active);
        let snapshot = if wipes(motion) {
            // Without the previous frame there is nothing to wipe: the new scene shows at once.
            match &self.snapshot {
                Some((serial, frame)) if *serial == motion.get_dissolve_serial() => Some((*serial, frame)),
                _ => None,
            }
        } else {
            None
        };
        let Some(texture) = texture.filter(|_| snapshot.is_some() || !wipes(motion)) else {
            self.source = None;
            self.alpha = Vec::new();
            self.image = None;
            if !active {
                self.snapshot = None;
            }
            return;
        };
        let alpha = motion.get_dissolve_alpha().clamp(0.0, 1.0);
        // Original engine: wait=4 (sub_425130) alpha-tests mask + 255 - 255t against 255, a
        // hard-edged wipe; wait=5 (sub_424C10) blends mask + 255 - 510t. The top bit marks hard.
        let hard = motion.get_dissolve_type() == DissolveType::MaskFadeIn;
        let phase = if hard {
            0x8000 | ((1.0 - alpha) * 255.0) as u16
        } else {
            (alpha * 510.0 + 0.5) as u16
        };
        let serial = snapshot.map(|(serial, _)| serial);
        let same = self.source.as_ref().map_or(false, |(generation, path, s)| {
            *generation == graph.generation && path == &graph.texture_path && *s == serial
        });
        if same && self.phase == phase {
            return;
        }
        if !same {
            let (mw, mh) = texture.dimensions();
            let Some((_, _, _, _, alpha)) = graph.export_alpha_mask() else {
                self.image = None;
                return;
            };
            match snapshot {
                Some((_, frame)) => {
                    // The frame's resolution, the mask scaled to it.
                    let (w, h) = (frame.width(), frame.height());
                    let mut scaled = Vec::with_capacity((w * h) as usize);
                    for y in 0..h {
                        let row = (y as u64 * mh as u64 / h.max(1) as u64) as usize * mw as usize;
                        for x in 0..w {
                            let col = (x as u64 * mw as u64 / w.max(1) as u64) as usize;
                            scaled.push(alpha.get(row + col).copied().unwrap_or(0));
                        }
                    }
                    self.alpha = scaled;
                    self.image = Some(DynamicImage::ImageRgba8(frame.clone()));
                }
                None => {
                    self.alpha = alpha;
                    self.image = Some(DynamicImage::ImageRgba8(RgbaImage::new(mw, mh)));
                }
            }
            self.source = Some((graph.generation, graph.texture_path.clone(), serial));
        }
        let mut lut = [0u8; 256];
        for (alpha, value) in lut.iter_mut().enumerate() {
            *value = if hard {
                if alpha as u16 >= phase & 0xff { 255 } else { 0 }
            } else {
                (alpha as i32 + i32::from(phase) - 255).clamp(0, 255) as u8
            };
        }
        let rgba = self.image.as_mut().unwrap().as_mut_rgba8().unwrap();
        for (pixel, alpha) in rgba.as_mut().chunks_exact_mut(4).zip(&self.alpha) {
            pixel[3] = lut[*alpha as usize];
        }
        self.phase = phase;
        self.generation = self.generation.wrapping_add(1);
    }
}
