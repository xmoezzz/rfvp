use proc_macro2::{Ident, Punct, Spacing, Span};
use quote::TokenStreamExt;

/// Vertex Format for a [`VertexAttribute`] (input).
///
/// Corresponds to [WebGPU `GPUVertexFormat`](
/// https://gpuweb.github.io/gpuweb/#enumdef-gpuvertexformat).
// copied from wgpu to avoid a dependency
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub enum VertexFormat {
    /// Two unsigned bytes (u8). `vec2<u32>` in shaders.
    Uint8x2 = 0,
    /// Four unsigned bytes (u8). `vec4<u32>` in shaders.
    Uint8x4 = 1,
    /// Two signed bytes (i8). `vec2<i32>` in shaders.
    Sint8x2 = 2,
    /// Four signed bytes (i8). `vec4<i32>` in shaders.
    Sint8x4 = 3,
    /// Two unsigned bytes (u8). [0, 255] converted to float [0, 1] `vec2<f32>` in shaders.
    Unorm8x2 = 4,
    /// Four unsigned bytes (u8). [0, 255] converted to float [0, 1] `vec4<f32>` in shaders.
    Unorm8x4 = 5,
    /// Two signed bytes (i8). [-127, 127] converted to float [-1, 1] `vec2<f32>` in shaders.
    Snorm8x2 = 6,
    /// Four signed bytes (i8). [-127, 127] converted to float [-1, 1] `vec4<f32>` in shaders.
    Snorm8x4 = 7,
    /// Two unsigned shorts (u16). `vec2<u32>` in shaders.
    Uint16x2 = 8,
    /// Four unsigned shorts (u16). `vec4<u32>` in shaders.
    Uint16x4 = 9,
    /// Two signed shorts (i16). `vec2<i32>` in shaders.
    Sint16x2 = 10,
    /// Four signed shorts (i16). `vec4<i32>` in shaders.
    Sint16x4 = 11,
    /// Two unsigned shorts (u16). [0, 65535] converted to float [0, 1] `vec2<f32>` in shaders.
    Unorm16x2 = 12,
    /// Four unsigned shorts (u16). [0, 65535] converted to float [0, 1] `vec4<f32>` in shaders.
    Unorm16x4 = 13,
    /// Two signed shorts (i16). [-32767, 32767] converted to float [-1, 1] `vec2<f32>` in shaders.
    Snorm16x2 = 14,
    /// Four signed shorts (i16). [-32767, 32767] converted to float [-1, 1] `vec4<f32>` in shaders.
    Snorm16x4 = 15,
    /// Two half-precision floats (no Rust equiv). `vec2<f32>` in shaders.
    Float16x2 = 16,
    /// Four half-precision floats (no Rust equiv). `vec4<f32>` in shaders.
    Float16x4 = 17,
    /// One single-precision float (f32). `f32` in shaders.
    Float32 = 18,
    /// Two single-precision floats (f32). `vec2<f32>` in shaders.
    Float32x2 = 19,
    /// Three single-precision floats (f32). `vec3<f32>` in shaders.
    Float32x3 = 20,
    /// Four single-precision floats (f32). `vec4<f32>` in shaders.
    Float32x4 = 21,
    /// One unsigned int (u32). `u32` in shaders.
    Uint32 = 22,
    /// Two unsigned ints (u32). `vec2<u32>` in shaders.
    Uint32x2 = 23,
    /// Three unsigned ints (u32). `vec3<u32>` in shaders.
    Uint32x3 = 24,
    /// Four unsigned ints (u32). `vec4<u32>` in shaders.
    Uint32x4 = 25,
    /// One signed int (i32). `i32` in shaders.
    Sint32 = 26,
    /// Two signed ints (i32). `vec2<i32>` in shaders.
    Sint32x2 = 27,
    /// Three signed ints (i32). `vec3<i32>` in shaders.
    Sint32x3 = 28,
    /// Four signed ints (i32). `vec4<i32>` in shaders.
    Sint32x4 = 29,
    /// One double-precision float (f64). `f32` in shaders. Requires [`Features::VERTEX_ATTRIBUTE_64BIT`].
    Float64 = 30,
    /// Two double-precision floats (f64). `vec2<f32>` in shaders. Requires [`Features::VERTEX_ATTRIBUTE_64BIT`].
    Float64x2 = 31,
    /// Three double-precision floats (f64). `vec3<f32>` in shaders. Requires [`Features::VERTEX_ATTRIBUTE_64BIT`].
    Float64x3 = 32,
    /// Four double-precision floats (f64). `vec4<f32>` in shaders. Requires [`Features::VERTEX_ATTRIBUTE_64BIT`].
    Float64x4 = 33,
    /// Three unsigned 10-bit integers and one 2-bit integer, packed into a 32-bit integer (u32). [0, 1024] converted to float [0, 1] `vec4<f32>` in shaders.
    #[allow(unused)]
    Unorm10_10_10_2 = 34,
}

impl quote::ToTokens for VertexFormat {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        //tokens.append_separated(&["wgpu", "VertexFormat", "Float32x2"], "::");
        tokens.append(Ident::new("wgpu", Span::call_site()));
        tokens.append(Punct::new(':', Spacing::Joint));
        tokens.append(Punct::new(':', Spacing::Alone));
        tokens.append(Ident::new("VertexFormat", Span::call_site()));
        tokens.append(Punct::new(':', Spacing::Joint));
        tokens.append(Punct::new(':', Spacing::Alone));
        tokens.append(Ident::new(
            format!("{:?}", self).as_str(),
            Span::call_site(),
        ));
    }
}

/// Whether a vertex buffer is indexed by vertex or by instance.
// copied from wgpu to avoid a dependency
#[derive(Copy, Clone, Debug, Default, Hash, Eq, PartialEq)]
pub enum VertexStepMode {
    /// Vertex data is advanced every vertex.
    #[default]
    Vertex = 0,
    /// Vertex data is advanced every instance.
    #[allow(unused)]
    Instance = 1,
}

impl quote::ToTokens for VertexStepMode {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        //tokens.append_separated(&["wgpu", "VertexFormat", "Float32x2"], "::");
        tokens.append(Ident::new("wgpu", Span::call_site()));
        tokens.append(Punct::new(':', Spacing::Joint));
        tokens.append(Punct::new(':', Spacing::Alone));
        tokens.append(Ident::new("VertexStepMode", Span::call_site()));
        tokens.append(Punct::new(':', Spacing::Joint));
        tokens.append(Punct::new(':', Spacing::Alone));
        tokens.append(Ident::new(
            format!("{:?}", self).as_str(),
            Span::call_site(),
        ));
    }
}
