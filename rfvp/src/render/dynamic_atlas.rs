use std::{
    ops::Deref,
    sync::{Mutex, RwLock},
};

use bevy_utils::{Entry, HashMap};
use glam::{vec2, Vec2};
use rfvp_render::{GpuCommonResources, TextureBindGroup};
use tracing::info;
use usvg::{tiny_skia_path, NodeKind, NormalizedF32, TreeParsing};

use crate::render::overlay::{OverlayCollector, OverlayVisitable};

pub trait ImageProvider {
    const IMAGE_FORMAT: wgpu::TextureFormat;
    const MIPMAP_LEVELS: u32;
    type Id: Copy + Eq + std::hash::Hash;

    fn get_image(&self, id: Self::Id) -> (Vec<Vec<u8>>, (u32, u32));
}

struct AtlasAllocation {
    allocation: etagere::Allocation,
    ref_count: usize,
}

impl AtlasAllocation {
    pub fn as_atlas_image(&self) -> AtlasImage {
        let pos = self.allocation.rectangle.min;
        let size = self.allocation.rectangle.size();

        AtlasImage {
            position: vec2(pos.x as f32, pos.y as f32),
            size: vec2(size.width as f32, size.height as f32),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasImage {
    pub position: Vec2,
    pub size: Vec2,
}

/// Dynamic texture atlas, (for now) used for text rendering.
pub struct DynamicAtlas<P: ImageProvider> {
    image_provider: P,

    label: String,

    // TODO: support multiple atlas pages
    texture: wgpu::Texture,
    texture_bind_group: TextureBindGroup,
    texture_size: (u32, u32),

    // TODO: I am not sure that this "split" locking can't cause deadlocks
    allocator: Mutex<etagere::BucketedAtlasAllocator>,
    /// These are the images that are currently in the atlas and cannot be evicted.
    active_allocations: RwLock<HashMap<P::Id, AtlasAllocation>>,
    /// These are images still in the atlas, but can be evicted.
    eviction_ready: Mutex<HashMap<P::Id, etagere::Allocation>>,
}

impl<P: ImageProvider> DynamicAtlas<P> {
    pub fn new(
        resources: &GpuCommonResources,
        image_provider: P,
        texture_size: (u32, u32),
        label: Option<&str>,
    ) -> Self {
        let label = label
            .map(|s| format!("{} DynamicAtlas", s))
            .unwrap_or_else(|| "DynamicAtlas".to_string());

        let texture = resources.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("{} Texture", label)),
            size: wgpu::Extent3d {
                width: texture_size.0,
                height: texture_size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: P::MIPMAP_LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: P::IMAGE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // TODO: make sampler configurable
        let texture_sampler = resources.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{} Sampler", label)),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_bind_group = TextureBindGroup::new(
            resources,
            &texture_view,
            &texture_sampler,
            Some(&format!("{} TextureBindGroup", label)),
        );

        let allocator = etagere::BucketedAtlasAllocator::with_options(
            etagere::Size::new(
                texture_size.0.try_into().unwrap(),
                texture_size.1.try_into().unwrap(),
            ),
            &etagere::AllocatorOptions {
                alignment: etagere::Size::new(8, 8), // TODO: make this configurable
                vertical_shelves: false,
                num_columns: 1,
            },
        );

        Self {
            image_provider,
            label,
            texture,
            texture_bind_group,
            texture_size,
            allocator: Mutex::new(allocator),
            active_allocations: RwLock::new(HashMap::default()),
            eviction_ready: Mutex::new(HashMap::default()),
        }
    }

    pub fn texture_bind_group(&self) -> &TextureBindGroup {
        &self.texture_bind_group
    }

    pub fn texture_size(&self) -> (u32, u32) {
        self.texture_size
    }

    /// Gets an image from the atlas, or adds it if it's not already there.
    /// Increases the ref count of the image.
    pub fn get_image(&self, resources: &GpuCommonResources, id: P::Id) -> Option<AtlasImage> {
        let mut active_allocations = self.active_allocations.write().unwrap();

        let entry = active_allocations.entry(id);

        let allocation: &AtlasAllocation = match entry {
            Entry::Occupied(entry) => {
                let allocation = entry.into_mut();
                allocation.ref_count += 1;
                allocation
            }
            Entry::Vacant(entry) => {
                let mut eviction_ready = self.eviction_ready.lock().unwrap();
                if let Some(allocation) = eviction_ready.remove(&id) {
                    // The image is already allocated, but not in use, so we can restore it
                    entry.insert(AtlasAllocation {
                        allocation,
                        ref_count: 1,
                    })
                } else {
                    // The image is not in atlas. We need to actually upload it to GPU
                    let (mip_data, (width, height)) = self.image_provider.get_image(id);

                    assert_eq!(mip_data.len(), P::MIPMAP_LEVELS as usize);

                    // First, find a place to put it
                    // no compressed textures support for now
                    assert_eq!(P::IMAGE_FORMAT.block_dimensions(), (1, 1));
                    let block_size = P::IMAGE_FORMAT.block_size(None).unwrap();

                    let allocation = {
                        let mut allocator = self.allocator.lock().unwrap();
                        if let Some(alloc) = allocator.allocate(etagere::Size::new(
                            width.try_into().unwrap(),
                            height.try_into().unwrap(),
                        )) {
                            alloc
                        } else {
                            // seems like we are out of space
                            // we can evict unused images to make space
                            for (_id, alloc) in eviction_ready.drain() {
                                allocator.deallocate(alloc.id);
                            }
                            info!(
                                label = self.label,
                                "Evicted all atlas images to make space for new ones, free space: {:.2}%", 
                                100.0 * allocator.free_space() as f32 / allocator.size().area() as f32
                            );

                            // allocator
                            //     .dump_svg(&mut std::fs::File::create("atlas_dump.svg").unwrap())
                            //     .unwrap();

                            if let Some(alloc) = allocator.allocate(etagere::Size::new(
                                width.try_into().unwrap(),
                                height.try_into().unwrap(),
                            )) {
                                alloc
                            } else {
                                panic!("Failed to allocate atlas space for image, even after evicting all unused images");
                            }
                        }
                    };

                    let x: u32 = allocation.rectangle.min.x.try_into().unwrap();
                    let y: u32 = allocation.rectangle.min.y.try_into().unwrap();

                    for (mip_level, data) in (0..P::MIPMAP_LEVELS).zip(mip_data) {
                        let mip_scale = 1 << mip_level;

                        assert_eq!(
                            data.len(),
                            (width * height / mip_scale / mip_scale) as usize * block_size as usize
                        );

                        // Upload the image to the atlas
                        let texture_copy_view = wgpu::ImageCopyTexture {
                            texture: &self.texture,
                            mip_level,
                            origin: wgpu::Origin3d {
                                x: x / mip_scale,
                                y: y / mip_scale,
                                z: 0,
                            },
                            aspect: Default::default(),
                        };

                        resources.queue.write_texture(
                            texture_copy_view,
                            &data,
                            wgpu::ImageDataLayout {
                                offset: 0,
                                bytes_per_row: Some(width * block_size / mip_scale),
                                rows_per_image: Some(height / mip_scale),
                            },
                            wgpu::Extent3d {
                                width: width / mip_scale,
                                height: height / mip_scale,
                                depth_or_array_layers: 1,
                            },
                        );
                    }

                    entry.insert(AtlasAllocation {
                        allocation,
                        ref_count: 1,
                    })
                }
            }
        };

        Some(allocation.as_atlas_image())
    }

    #[allow(unused)]
    pub fn peek_image(&self, id: P::Id) -> Option<AtlasImage> {
        Some(
            self.active_allocations
                .read()
                .unwrap()
                .get(&id)?
                .as_atlas_image(),
        )
    }

    pub fn free_image(&self, id: P::Id) {
        let mut active_allocations = self.active_allocations.write().unwrap();

        let allocation = active_allocations
            .get_mut(&id)
            .expect("Attempt to free an image not in atlas");

        allocation.ref_count -= 1;

        if allocation.ref_count == 0 {
            self.eviction_ready
                .lock()
                .unwrap()
                .insert(id, allocation.allocation);
            active_allocations.remove(&id);
        }
    }

    pub fn provider(&self) -> &P {
        &self.image_provider
    }

    #[allow(unused)]
    pub fn provider_mut(&mut self) -> &mut P {
        &mut self.image_provider
    }

    pub fn free_space(&self) -> f32 {
        let allocator = self.allocator.lock().unwrap();
        allocator.free_space() as f32 / allocator.size().area() as f32
    }
}

// fn stroke_path(path: &usvg::PathData) {}

fn fill_path(path: &tiny_skia_path::Path, fill: egui::Color32) -> egui::Shape {
    let mut points = Vec::new();

    let mut iter = path.segments();
    if let Some(first) = iter.next() {
        match first {
            tiny_skia_path::PathSegment::MoveTo(p) => {
                points.push(egui::Pos2::new(p.x, p.y));
            }
            _ => unimplemented!("First segment of path must be MoveTo"),
        }
    } else {
        return egui::Shape::Noop;
    }

    for segment in &mut iter {
        match segment {
            tiny_skia_path::PathSegment::LineTo(p) => {
                points.push(egui::Pos2::new(p.x, p.y));
            }
            tiny_skia_path::PathSegment::Close => break,
            e => panic!("Unexpected segment: {:?}", e),
        }
    }
    assert!(
        matches!(iter.next(), None),
        "ClosePath can only be the last segment"
    );

    egui::Shape::Path(egui::epaint::PathShape {
        points,
        closed: true,
        fill,
        stroke: egui::epaint::PathStroke::NONE,
    })
}

fn stroke_path(path: &tiny_skia_path::Path, width: f32, color: egui::Color32) -> egui::Shape {
    let mut points = Vec::new();

    let mut iter = path.segments();
    if let Some(first) = iter.next() {
        match first {
            tiny_skia_path::PathSegment::MoveTo(p) => {
                points.push(egui::Pos2::new(p.x, p.y));
            }
            _ => unimplemented!("First segment of path must be MoveTo"),
        }
    } else {
        return egui::Shape::Noop;
    }

    let mut closed = false;
    for segment in &mut iter {
        match segment {
            tiny_skia_path::PathSegment::LineTo(p) => {
                points.push(egui::Pos2::new(p.x, p.y));
            }
            tiny_skia_path::PathSegment::Close => {
                closed = true;
                break;
            }
            e => panic!("Unexpected segment: {:?}", e),
        }
    }
    assert!(
        matches!(iter.next(), None),
        "ClosePath can only be the last segment"
    );

    egui::Shape::Path(egui::epaint::PathShape {
        points,
        closed,
        fill: egui::Color32::TRANSPARENT,
        stroke: egui::epaint::PathStroke::new(width, color),
    })
}

fn convert_path(
    transform: usvg::Transform,
    path: &usvg::Path,
    extra_opacity: NormalizedF32,
) -> egui::Shape {
    let data = path.data.deref().clone().transform(transform).unwrap();

    let fill = if let Some(fill) = &path.fill {
        // egui supports only convex fills anyways
        assert_eq!(fill.rule, usvg::FillRule::NonZero);
        if let usvg::Paint::Color(color) = fill.paint {
            let color = egui::Color32::from_rgba_unmultiplied(
                color.red,
                color.green,
                color.blue,
                (fill.opacity * extra_opacity).to_u8(),
            );
            fill_path(&data, color)
        } else {
            todo!("non-solid svg fill")
        }
    } else {
        egui::Shape::Noop
    };
    let stroke = if let Some(stroke) = &path.stroke {
        assert_eq!(stroke.dasharray, None);
        // no handling of linecap/linejoin because egui doesn't expose them

        if let usvg::Paint::Color(color) = stroke.paint {
            let color = egui::Color32::from_rgba_unmultiplied(
                color.red,
                color.green,
                color.blue,
                (stroke.opacity * extra_opacity).to_u8(),
            );
            stroke_path(&data, stroke.width.get(), color)
        } else {
            todo!("non-solid svg stroke")
        }
    } else {
        egui::Shape::Noop
    };

    match path.paint_order {
        usvg::PaintOrder::FillAndStroke => egui::Shape::Vec(vec![fill, stroke]),
        usvg::PaintOrder::StrokeAndFill => egui::Shape::Vec(vec![stroke, fill]),
    }
}

impl<P: ImageProvider> OverlayVisitable for DynamicAtlas<P> {
    fn visit_overlay(&self, collector: &mut OverlayCollector) {
        collector.overlay(
            &self.label,
            |ctx, _top_left| {
                egui::Window::new(&self.label)
                    .resizable(true)
                    .default_width(256.0)
                    .default_height(256.0 + 32.0)
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "Atlas size: {}x{}\nFree space: {:.2}%",
                            self.texture_size.0,
                            self.texture_size.1,
                            100.0 * self.free_space()
                        ));

                        let mut svg_bytes = Vec::new();
                        self.allocator
                            .lock()
                            .unwrap()
                            .dump_svg(&mut svg_bytes)
                            .unwrap();

                        let svg =
                            usvg::Tree::from_data(&svg_bytes, &usvg::Options::default()).unwrap();

                        let svg_size = vec2(svg.size.width(), svg.size.height());
                        let min_scale = 1.0 / 12.0;
                        let min_size = svg_size * min_scale;

                        let view_box = svg.view_box;

                        let mut size = ui.available_size();
                        size.x = size.x.max(min_size.x);
                        size.y = size.y.max(min_size.y);
                        let (_id, mut rect) = ui.allocate_space(size);

                        // fiddle with rect to make aspect ratio correct
                        let aspect_ratio = min_size.x / min_size.y;
                        let rect_aspect_ratio = rect.width() / rect.height();
                        // shrinking the size as needed, but keeping the center of the rect the same
                        if aspect_ratio > rect_aspect_ratio {
                            let new_height = rect.width() / aspect_ratio;
                            let old_height = rect.height();
                            rect.min.y += (old_height - new_height) / 2.0;
                            rect.max.y -= (old_height - new_height) / 2.0;
                        } else {
                            let new_width = rect.height() * aspect_ratio;
                            let old_width = rect.width();
                            rect.min.x += (old_width - new_width) / 2.0;
                            rect.max.x -= (old_width - new_width) / 2.0;
                        }

                        // transform from svg's coordinate system to egui's (after positioning the widget)
                        let transform = usvg::Transform::default()
                            .pre_translate(rect.min.x, rect.min.y)
                            .pre_scale(rect.width(), rect.height())
                            .pre_scale(1.0 / view_box.rect.width(), 1.0 / view_box.rect.height())
                            .pre_translate(-view_box.rect.x(), -view_box.rect.y());

                        let painter = ui.painter().with_clip_rect(rect);
                        for node in svg.root.descendants() {
                            match node.borrow().deref() {
                                NodeKind::Group(_g) => {}
                                NodeKind::Path(p) => {
                                    assert_eq!(p.transform, usvg::Transform::default());
                                    if p.visibility != usvg::Visibility::Visible {
                                        continue;
                                    }

                                    painter.add(convert_path(
                                        transform,
                                        p,
                                        NormalizedF32::new_clamped(0.5),
                                    ));
                                }
                                NodeKind::Image(_) => todo!(),
                                NodeKind::Text(_) => todo!(),
                            }
                        }
                    });
            },
            false,
        );
    }
}
