// here we create an abstraction over wgpu which makes it look more like shin's render abstraction over nvn
// an important departure is not using global variables, but making all the arguments explicit (helped by a builder pattern with typestates (maybe))

mod buffer;
mod resources;

use glam::{Mat4, Vec4};

use crate::{
    new_render::buffer::VertexBufferSliceReference,
    vertices::{PosColVertex, PosVertex4},
    Vertex,
};

// TODO: provide arguments required for each shape
#[derive(Debug, Copy, Clone)]
enum LayerShaderShape {
    Layer,
    LayerDiscard,
    LayerPremultiply,
    Mask,
}

#[derive(Debug, Copy, Clone)]
enum LayerShaderOperation {
    Default,
    Mono,
    Fill,
    Fill2,
    Negative,
    Gamma,
}

#[derive(Debug, Copy, Clone)]
enum WiperKind {
    Default,
    Mask,
    Wave,
    Ripple,
    Whirl,
    Glass,
}

#[derive(Debug, Copy, Clone)]
enum RenderProgram {
    Clear,
    Fill,
    Sprite,
    Font,
    FontBorder,
    Button,
    Blend,
    Window,

    Layer(LayerShaderShape, LayerShaderOperation),

    Dissolve,
    TapEffect,
    Movie,
    MovieAlpha,

    Wiper(WiperKind),

    Mosaic,
    Blur,
    ZoomBlur,
    Raster,
    Ripple,
    Breakup,

    Charicon0,
    Charicon1,
    Charicon2,
    Charicon3,
    Test,
}

#[derive(Debug)]
enum RenderProgramWithArguments {
    // TODO: provide arguments required for each program
    Clear {
        // TODO: it should actually be some kind of "vertex source" type, as we want to allow index buffers
        // TODO: fix the trait impl
        // vertices: VertexBufferSliceReference<PosVertex4>,
        color: Vec4,
    },
    Fill {
        // TODO: fix the trait impl
        // vertices: VertexBufferSliceReference<PosColVertex>,
        transform: Mat4,
    },
    Sprite {},
    Font {},
    FontBorder {},
    Button {},
    Blend {},
    Window {},

    Layer {
        shape: LayerShaderShape,
        operation: LayerShaderOperation,
    },

    Dissolve {},
    TapEffect {},
    Movie {},
    MovieAlpha {},

    Wiper {
        kind: WiperKind,
    },

    Mosaic {},
    Blur {},
    ZoomBlur {},
    Raster {},
    Ripple {},
    Breakup {},

    Charicon0 {},
    Charicon1 {},
    Charicon2 {},
    Charicon3 {},
    Test {},
}

impl RenderProgramWithArguments {
    fn get_render_program(&self) -> RenderProgram {
        match *self {
            RenderProgramWithArguments::Clear { .. } => RenderProgram::Clear,
            RenderProgramWithArguments::Fill { .. } => RenderProgram::Fill,
            RenderProgramWithArguments::Sprite { .. } => RenderProgram::Sprite,
            RenderProgramWithArguments::Font { .. } => RenderProgram::Font,
            RenderProgramWithArguments::FontBorder { .. } => RenderProgram::FontBorder,
            RenderProgramWithArguments::Button { .. } => RenderProgram::Button,
            RenderProgramWithArguments::Blend { .. } => RenderProgram::Blend,
            RenderProgramWithArguments::Window { .. } => RenderProgram::Window,
            RenderProgramWithArguments::Layer {
                shape, operation, ..
            } => RenderProgram::Layer(shape, operation),
            RenderProgramWithArguments::Dissolve { .. } => RenderProgram::Dissolve,
            RenderProgramWithArguments::TapEffect { .. } => RenderProgram::TapEffect,
            RenderProgramWithArguments::Movie { .. } => RenderProgram::Movie,
            RenderProgramWithArguments::MovieAlpha { .. } => RenderProgram::MovieAlpha,
            RenderProgramWithArguments::Wiper { .. } => RenderProgram::Wiper(WiperKind::Default),
            RenderProgramWithArguments::Mosaic { .. } => RenderProgram::Mosaic,
            RenderProgramWithArguments::Blur { .. } => RenderProgram::Blur,
            RenderProgramWithArguments::ZoomBlur { .. } => RenderProgram::ZoomBlur,
            RenderProgramWithArguments::Raster { .. } => RenderProgram::Raster,
            RenderProgramWithArguments::Ripple { .. } => RenderProgram::Ripple,
            RenderProgramWithArguments::Breakup { .. } => RenderProgram::Breakup,
            RenderProgramWithArguments::Charicon0 { .. } => RenderProgram::Charicon0,
            RenderProgramWithArguments::Charicon1 { .. } => RenderProgram::Charicon1,
            RenderProgramWithArguments::Charicon2 { .. } => RenderProgram::Charicon2,
            RenderProgramWithArguments::Charicon3 { .. } => RenderProgram::Charicon3,
            RenderProgramWithArguments::Test { .. } => RenderProgram::Test,
        }
    }
}

#[derive(Debug, Default, Copy, Clone)]
enum DepthFunction {
    Never,
    Less,
    Equal,
    LessOrEqual,
    Greater,
    NotEqual,
    GreaterOrEqual,
    #[default]
    Always,
}

#[derive(Debug, Default, Copy, Clone)]
struct DepthState {
    function: DepthFunction,
    write_enable: bool,
}

#[derive(Debug, Default, Copy, Clone)]
enum StencilFunction {
    Never,
    Less,
    Equal,
    LessOrEqual,
    Greater,
    NotEqual,
    GreaterOrEqual,
    #[default]
    Always,
}

#[derive(Debug, Default, Copy, Clone)]
enum StencilOperation {
    #[default]
    Keep,
    Zero,
    Replace,
    Increment,
    Decrement,
    Invert,
    IncrementWrap,
    DecrementWrap,
}

#[derive(Debug, Copy, Clone)]
struct StencilState {
    function: StencilFunction,
    stencil_fail_operation: StencilOperation,
    depth_fail_operation: StencilOperation,
    pass_operation: StencilOperation,
    stencil_mask: u8,
    stencil_value_mask: u8,
    stencil_reference: u8,
}

impl Default for StencilState {
    fn default() -> Self {
        Self {
            function: StencilFunction::default(),
            stencil_fail_operation: StencilOperation::default(),
            depth_fail_operation: StencilOperation::default(),
            pass_operation: StencilOperation::default(),
            stencil_mask: 0xff,
            stencil_value_mask: 0xff,
            stencil_reference: 0,
        }
    }
}

#[derive(Debug, Default, Copy, Clone)]
struct DepthStencilState {
    depth: DepthState,
    stencil: StencilState,
}

#[derive(Debug, Default, Copy, Clone)]
enum ColorBlendType {
    NoColor,
    #[default]
    Opaque,
    Layer1,
    Layer2,
    Layer3,
    LayerPremultiplied1,
    LayerPremultiplied2,
    LayerPremultiplied3,
}

struct PipelineCacheKey {
    program: RenderProgram,
}

struct RenderRequest {
    program: RenderProgram,
    depth_stencil: DepthStencilState,
    // is this actually used??
    // cull_faces
    color_blend_type: ColorBlendType,
    // TODO: we actually want to typecheck the type of vertices
    // vertex_buffer: BufferHandle,
    // index_buffer: BufferHandle,
}
