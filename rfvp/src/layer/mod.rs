mod bustup_layer;
mod layer_group;
mod message_layer;
mod movie_layer;
mod null_layer;
mod page_layer;
mod picture_layer;
mod root_layer_group;
mod screen_layer;
mod tile_layer;
mod wobbler;

use std::f32::consts::PI;

pub use bustup_layer::BustupLayer;
use derivative::Derivative;
use derive_more::From;
use enum_dispatch::enum_dispatch;
use enum_map::{enum_map, EnumMap};
use glam::{vec3, Mat4};
pub use layer_group::LayerGroup;
pub use message_layer::{MessageLayer, MessageboxTextures};
pub use movie_layer::MovieLayer;
pub use null_layer::NullLayer;
pub use page_layer::PageLayer;
pub use picture_layer::PictureLayer;
pub use root_layer_group::RootLayerGroup;
pub use screen_layer::ScreenLayer;
use rfvp_audio::AudioManager;
use rfvp_core::{
    format::scenario::{
        info::{BustupInfoItem, MovieInfoItem, PictureInfoItem},
        instruction_elements::UntypedNumberArray,
        Scenario,
    },
    time::{Ticks, Tweener},
    vm::command::types::{LayerProperty, LayerType},
};
use rfvp_render::{GpuCommonResources, Renderable};
pub use tile_layer::TileLayer;
use tracing::{debug, warn};

use crate::{
    asset::{bustup::Bustup, movie::Movie, picture::Picture, AnyAssetServer},
    layer::wobbler::Wobbler,
    update::{Updatable, UpdateContext},
};

fn initial_values() -> EnumMap<LayerProperty, i32> {
    enum_map! {
        v => v.initial_value()
    }
}

pub struct LayerProperties {
    properties: EnumMap<LayerProperty, Tweener>,
    wobbler_x: Wobbler,
    wobbler_y: Wobbler,
    wobbler_alpha: Wobbler,
    wobbler_rotation: Wobbler,
    wobbler_scale_x: Wobbler,
    wobbler_scale_y: Wobbler,
}

impl LayerProperties {
    pub fn new() -> Self {
        Self {
            properties: initial_values().map(|_, v| Tweener::new(v as f32)),
            wobbler_x: Wobbler::new(),
            wobbler_y: Wobbler::new(),
            wobbler_alpha: Wobbler::new(),
            wobbler_rotation: Wobbler::new(),
            wobbler_scale_x: Wobbler::new(),
            wobbler_scale_y: Wobbler::new(),
        }
    }

    pub fn get_property_value(&self, property: LayerProperty) -> f32 {
        self.properties[property].value()
    }
    #[allow(unused)]
    pub fn property_tweener(&self, property: LayerProperty) -> &Tweener {
        &self.properties[property]
    }

    pub fn property_tweener_mut(&mut self, property: LayerProperty) -> &mut Tweener {
        &mut self.properties[property]
    }

    pub fn init(&mut self) {
        for (prop, val) in initial_values() {
            self.properties[prop].fast_forward_to(val as f32);
        }
    }

    pub fn compute_transform(&self, base_transform: Mat4) -> Mat4 {
        macro_rules! get {
            (Zero) => {
                0.0
            };
            ($property:ident) => {
                self.get_property_value(LayerProperty::$property)
            };
            ($x_property:ident, $y_property:ident, $z_property:ident) => {
                vec3(get!($x_property), get!($y_property), get!($z_property))
            };
        }

        macro_rules! wobble {
            ($wobbler_name:ident, $amplitude:ident, $bias:ident) => {
                self.$wobbler_name.value() * get!($amplitude) + get!($bias)
            };
        }

        // TODO: actually use all the properties

        let transforms = [
            // apply scale
            Mat4::from_translation(-get!(ScaleOriginX, ScaleOriginY, Zero)),
            Mat4::from_scale(vec3(
                get!(ScaleX) / 1000.0 * get!(ScaleX2) / 1000.0
                    * wobble!(wobbler_scale_x, WobbleScaleXAmplitude, WobbleScaleXBias)
                    / 1000.0,
                get!(ScaleY) / 1000.0 * get!(ScaleY2) / 1000.0
                    * wobble!(wobbler_scale_y, WobbleScaleYAmplitude, WobbleScaleYBias)
                    / 1000.0,
                1.0,
            )),
            Mat4::from_translation(get!(ScaleOriginX, ScaleOriginY, Zero)),
            // apply rotation
            Mat4::from_translation(-get!(RotationOriginX, RotationOriginY, Zero)),
            Mat4::from_rotation_z({
                let rotations = get!(Rotation)
                    + get!(Rotation2)
                    + wobble!(
                        wobbler_rotation,
                        WobbleRotationAmplitude,
                        WobbleRotationBias
                    );

                rotations / 1000.0 * 2.0 * PI
            }),
            Mat4::from_translation(get!(RotationOriginX, RotationOriginY, Zero)),
            // apply translation
            Mat4::from_translation(get!(TranslateX, TranslateY, Zero)),
            Mat4::from_translation(get!(TranslateX2, TranslateY2, Zero)),
            Mat4::from_translation(vec3(
                wobble!(wobbler_x, WobbleXAmplitude, WobbleXBias),
                wobble!(wobbler_y, WobbleYAmplitude, WobbleYBias),
                0.0,
            )),
            base_transform,
        ];

        transforms
            .into_iter()
            .fold(Mat4::IDENTITY, |acc, t| t * acc)
    }
}

impl Updatable for LayerProperties {
    fn update(&mut self, context: &UpdateContext) {
        let dt = context.time_delta_ticks();

        for property in self.properties.values_mut() {
            property.update(dt);
        }

        macro_rules! get {
            ($property:ident) => {
                self.get_property_value(LayerProperty::$property)
            };
        }

        macro_rules! get_ticks {
            ($property:ident) => {
                Ticks::from_f32(get!($property))
            };
        }

        macro_rules! wobble {
            ($wobbler_name:ident, $wobble_mode:ident, $wobble_period:ident) => {
                self.$wobbler_name
                    .update(dt, get!($wobble_mode), get_ticks!($wobble_period));
            };
        }

        wobble!(wobbler_x, WobbleXMode, WobbleXPeriod);
        wobble!(wobbler_y, WobbleYMode, WobbleYPeriod);
        wobble!(wobbler_alpha, WobbleAlphaMode, WobbleAlphaPeriod);
        wobble!(wobbler_rotation, WobbleRotationMode, WobbleRotationPeriod);
        wobble!(wobbler_scale_x, WobbleScaleXMode, WobbleScaleXPeriod);
        wobble!(wobbler_scale_y, WobbleScaleYMode, WobbleScaleYPeriod);
    }
}

/// Stores only target property values.
/// Used to implement save/load (to quickly restore the state of the scene).
#[derive(Debug, Clone)]
pub struct LayerPropertiesSnapshot {
    // The game can actually only set integer values
    // hence the the use of i32 instead of f32
    properties: EnumMap<LayerProperty, i32>,
}

impl LayerPropertiesSnapshot {
    pub fn new() -> Self {
        Self {
            properties: initial_values(),
        }
    }

    pub fn init(&mut self) {
        self.properties = initial_values();
    }

    #[allow(unused)]
    pub fn get_property(&self, property: LayerProperty) -> i32 {
        self.properties[property]
    }

    pub fn set_property(&mut self, property: LayerProperty, value: i32) {
        self.properties[property] = value;
    }
}

#[enum_dispatch]
pub trait Layer: Renderable + Updatable {
    fn properties(&self) -> &LayerProperties;
    fn properties_mut(&mut self) -> &mut LayerProperties;
}

#[enum_dispatch(Layer, Updatable)]
#[derive(Derivative)]
#[derivative(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum UserLayer {
    #[derivative(Debug = "transparent")]
    NullLayer,
    #[derivative(Debug = "transparent")]
    PictureLayer,
    #[derivative(Debug = "transparent")]
    BustupLayer,
    #[derivative(Debug = "transparent")]
    TileLayer,
    #[derivative(Debug = "transparent")]
    MovieLayer,
}

impl UserLayer {
    pub async fn load(
        resources: &GpuCommonResources,
        asset_server: &AnyAssetServer,
        audio_manager: &AudioManager,
        scenario: &Scenario,
        layer_ty: LayerType,
        params: UntypedNumberArray,
    ) -> Self {
        // TODO: this API is not ideal, as we are blocking the main thread for layer loading
        // ideally we want to mimic the API of LayerLoader in the original game
        match layer_ty {
            LayerType::Null => NullLayer::new().into(),
            LayerType::Tile => {
                let (tile_color, offset_x, offset_y, width, height, ..) = params;
                TileLayer::new(resources, tile_color, offset_x, offset_y, width, height).into()
            }
            LayerType::Picture => {
                let (pic_id, ..) = params;
                let pic_info @ PictureInfoItem { name, linked_cg_id } =
                    scenario.info_tables().picture_info(pic_id);
                debug!("Load picture: {} -> {} {}", pic_id, name, linked_cg_id);
                let pic = asset_server
                    .load::<Picture, _>(pic_info.path())
                    .await
                    .expect("Failed to load picture");
                PictureLayer::new(resources, pic, Some(name.to_string())).into()
            }
            LayerType::Bustup => {
                let (bup_id, ..) = params;
                let bup_info @ BustupInfoItem {
                    name,
                    emotion,
                    lipsync_character_id,
                } = scenario.info_tables().bustup_info(bup_id);
                debug!(
                    "Load bustup: {} -> {} {} {}",
                    bup_id, name, emotion, lipsync_character_id
                );
                let bup = asset_server
                    .load::<Bustup, _>(bup_info.path())
                    .await
                    .expect("Failed to load bustup");

                BustupLayer::new(resources, bup, Some(name.to_string()), emotion.as_str()).into()
            }
            LayerType::Movie => {
                let (movie_id, _volume, _flags, ..) = params;
                let movie_info @ MovieInfoItem {
                    name,
                    linked_picture_id,
                    flags,
                    linked_bgm_id,
                } = scenario.info_tables().movie_info(movie_id);
                debug!(
                    "Load movie: {} -> {} {} {} {}",
                    movie_id, name, linked_picture_id, flags, linked_bgm_id
                );
                let movie = asset_server
                    .load::<Movie, _>(movie_info.path())
                    .await
                    .expect("Failed to load movie");

                MovieLayer::new(resources, audio_manager, movie, Some(name.to_string())).into()
            }
            LayerType::Rain => {
                let (_always_zero, _min_distance, _max_distance, ..) = params;

                warn!("Loading NullLayer instead of RainLayer");
                NullLayer::new().into()
            }
            _ => {
                todo!("Layer type not implemented: {:?}", layer_ty);
            }
        }
    }
}

impl Renderable for UserLayer {
    fn render<'enc>(
        &'enc self,
        resources: &'enc GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'enc>,
        transform: Mat4,
        projection: Mat4,
    ) {
        match self {
            UserLayer::NullLayer(l) => l.render(resources, render_pass, transform, projection),
            UserLayer::PictureLayer(l) => l.render(resources, render_pass, transform, projection),
            UserLayer::BustupLayer(l) => l.render(resources, render_pass, transform, projection),
            UserLayer::TileLayer(l) => l.render(resources, render_pass, transform, projection),
            UserLayer::MovieLayer(l) => l.render(resources, render_pass, transform, projection),
        }
    }

    fn resize(&mut self, resources: &GpuCommonResources) {
        match self {
            UserLayer::NullLayer(l) => l.resize(resources),
            UserLayer::PictureLayer(l) => l.resize(resources),
            UserLayer::BustupLayer(l) => l.resize(resources),
            UserLayer::TileLayer(l) => l.resize(resources),
            UserLayer::MovieLayer(l) => l.resize(resources),
        }
    }
}

#[derive(From)]
pub enum AnyLayer<'a> {
    UserLayer(&'a UserLayer),
    RootLayerGroup(&'a RootLayerGroup),
    ScreenLayer(&'a ScreenLayer),
    PageLayer(&'a PageLayer),
    LayerGroup(&'a LayerGroup),
}

impl<'a> AnyLayer<'a> {
    #[allow(unused)]
    pub fn properties(&self) -> &LayerProperties {
        match self {
            Self::UserLayer(layer) => layer.properties(),
            Self::RootLayerGroup(layer) => layer.properties(),
            Self::ScreenLayer(layer) => layer.properties(),
            Self::PageLayer(layer) => layer.properties(),
            Self::LayerGroup(layer) => layer.properties(),
        }
    }
}

#[derive(From)]
pub enum AnyLayerMut<'a> {
    UserLayer(&'a mut UserLayer),
    RootLayerGroup(&'a mut RootLayerGroup),
    ScreenLayer(&'a mut ScreenLayer),
    PageLayer(&'a mut PageLayer),
    LayerGroup(&'a mut LayerGroup),
}

impl<'a> AnyLayerMut<'a> {
    #[allow(unused)]
    pub fn properties(&self) -> &LayerProperties {
        match self {
            Self::UserLayer(layer) => layer.properties(),
            Self::RootLayerGroup(layer) => layer.properties(),
            Self::ScreenLayer(layer) => layer.properties(),
            Self::PageLayer(layer) => layer.properties(),
            Self::LayerGroup(layer) => layer.properties(),
        }
    }

    pub fn properties_mut(&mut self) -> &mut LayerProperties {
        match self {
            Self::UserLayer(layer) => layer.properties_mut(),
            Self::RootLayerGroup(layer) => layer.properties_mut(),
            Self::ScreenLayer(layer) => layer.properties_mut(),
            Self::PageLayer(layer) => layer.properties_mut(),
            Self::LayerGroup(layer) => layer.properties_mut(),
        }
    }
}
