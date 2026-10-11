//! Offline image reduction shared by the low-resolution console converters.
//! DPID provenance and redistribution terms are in DPID-LICENSE.txt.

mod dpid;

use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, str::FromStr};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    Area,
    #[default]
    DpidBalanced,
}

impl FromStr for Filter {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "area" => Ok(Self::Area),
            "dpid-balanced" => Ok(Self::DpidBalanced),
            _ => Err("image filter must be area or dpid-balanced".into()),
        }
    }
}

pub fn ui_filter(resource: &str, requested: Filter) -> Filter {
    let normalized = resource.replace('\\', "/").to_ascii_lowercase();
    if normalized.starts_with("graph/") && !normalized.contains("mask") {
        requested
    } else {
        Filter::Area
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutProfile {
    pub design_width: Option<u32>,
    pub design_height: Option<u32>,
    pub assets: BTreeMap<String, LayoutRule>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutRule {
    pub scale: f64,
    pub offset_x: Option<i16>,
    pub offset_y: Option<i16>,
}

impl LayoutProfile {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let profile: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        for (name, rule) in &profile.assets {
            if !rule.scale.is_finite() || !(0.25..=4.0).contains(&rule.scale) {
                return Err(format!("invalid layout scale for {name}: {}", rule.scale).into());
            }
        }
        Ok(profile)
    }

    pub fn get(&self, name: &str) -> Option<&LayoutRule> {
        self.assets
            .get(&name.replace('\\', "/").to_ascii_lowercase())
    }

    pub fn validate_design(&self, width: u32, height: u32) -> Result<(), String> {
        if self.design_width.is_some_and(|w| w != width)
            || self.design_height.is_some_and(|h| h != height)
        {
            return Err("UI layout design resolution does not match the input game".into());
        }
        Ok(())
    }
}

impl LayoutRule {
    /// Placement remains in design coordinates, as expected by old_school loaders.
    pub fn patch_header(&self, header: &mut [u8]) {
        if let Some(x) = self.offset_x {
            header[12..14].copy_from_slice(&x.to_le_bytes());
        }
        if let Some(y) = self.offset_y {
            header[14..16].copy_from_slice(&y.to_le_bytes());
        }
    }
}

/// BGR/BGRA stays in the NVSG premultiplied representation. Every animation
/// frame uses the same geometry and filter; masks and 1-bit glyphs use Area.
pub fn resize_color_frames(
    input: &[u8],
    sw: usize,
    sh: usize,
    dw: usize,
    dh: usize,
    channels: usize,
    frames: usize,
    filter: Filter,
) -> Vec<u8> {
    assert!(matches!(channels, 3 | 4));
    assert_eq!(input.len(), sw * sh * channels * frames);
    let mut output = Vec::with_capacity(dw * dh * channels * frames);
    for frame in input.chunks_exact(sw * sh * channels) {
        let rgba: Vec<u8> = frame
            .chunks_exact(channels)
            .flat_map(|p| {
                let a = if channels == 4 { p[3] } else { 255 };
                [p[2].min(a), p[1].min(a), p[0].min(a), a]
            })
            .collect();
        let (lambda, mix) = match filter {
            Filter::Area => (0.0, 0.0),
            Filter::DpidBalanced => (0.5, 0.5),
        };
        let resized = dpid::resize(&rgba, sw, sh, dw, dh, lambda, mix);
        for p in resized.chunks_exact(4) {
            output.extend_from_slice(&[p[2], p[1], p[0]]);
            if channels == 4 {
                output.push(p[3]);
            }
        }
    }
    output
}

pub fn resize_rgba(input: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    dpid::resize(input, sw, sh, dw, dh, 0.5, 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_channel_order_are_preserved() {
        let mut input = [0, 0, 255, 255].repeat(24);
        input.extend([0, 255, 0, 255].repeat(24));
        let output = resize_color_frames(&input, 6, 4, 3, 2, 4, 2, Filter::DpidBalanced);
        assert_eq!(&output[..24], &[0, 0, 255, 255].repeat(6));
        assert_eq!(&output[24..], &[0, 255, 0, 255].repeat(6));
    }

    #[test]
    fn transparent_colors_do_not_leak() {
        let output = resize_color_frames(
            &[255, 99, 120, 0].repeat(16),
            4,
            4,
            2,
            2,
            4,
            1,
            Filter::DpidBalanced,
        );
        assert_eq!(output, [0; 16]);
    }
}
