// MIT License

// Copyright (c) 2022 BrindilleDeLaForet

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! ## Description
//! 
//! wrld is a easy, fast, and more secure way of writing buffer descriptor for wgpu renderpipeline
//! 
//! ## How it works ?
//! 
//! Wrld take derived structure and crate a VertexBufferLayout according to the attribute passed.
//! 
//! Wrld come with 3 macros :
//! - Desc
//! - DescInstance
//! - BufferData
//! 
//! ### Desc
//! 
//! Desc derive macro allow to create a VertexBufferLayout from a structure.
//! 
//! #### Example
//! ```
//! #[repr(C)]
//! #[derive(wrld::Desc)]
//! struct Vertex {
//!     #[f32x2(0)] position: [f32; 2],
//!     #[f32x4(1)] color: [f32; 4]
//! }
//! ```
//! 
//! #### Thing to know
//! - Desc will not handle data transformation. (bytemuck slice)
//! - Desc does not handle chaotic structure.
//! 
//! ### DescInstance
//! 
//! DescInstance is the same as the Desc macro. The only difference with DescInstance is that it change the vertex step mode but the result is the same.
//! 
//! #### Example
//! ```
//! #[derive(wrld::DescInstance)]
//! struct Vertex {
//!     #[f32x2(0)] position: [f32; 2],
//!     #[f32x4(1)] color: [f32; 4]
//! }
//! ```
//! 
//! ### Chaotic and ordered structure.
//! 
//! Before aboarding the next macro. We need to know what is the difference between chaotic and ordered structure type.
//! 
//! ### Chaotic structure
//! 
//! A chaotic structure is a structure that have attributes put anywhere in the struct
//! 
//! #### Example 
//! ```
//! #[derive(wrld::Desc)]
//! struct Vertex {
//!     #[f32x2(0)] position: [f32; 2],
//!     data: String,
//!     #[f32x4(1)] color: [f32; 4]
//! }
//! ```
//! If you try to cast slice with bytemuck on this structure. It will result in this.
//! ```
//! struct Vertex {
//!     position: [f32; 2],
//!     data: String
//! }
//! ``` 
//! And this is not good, because this is not the data that we are expecting to get from bytemuck.
//! 
//! A simple fix to that is to create a ordered structure and have one or multiple function that convert this structure to a ordered one or to sort this one.
//! 
//! ### Ordered structure
//! 
//! A ordered structure is a structure that put the attribute field on top of the structure.
//! 
//! #### Example
//! ```
//! #[derive(wrld::Desc)]
//! struct Vertex {
//!     #[f32x2(0)] position: [f32; 2],
//!     #[f32x4(1)] color: [f32; 4],
//!     data: String
//! }
//! ```
//! If you try to cast slice with bytemuck on this structure. It will result in this.
//! ```
//! struct Vertex {
//!     position: [f32; 2],
//!     color: [f32; 4]
//! }
//! ```
//! 
//! While this technique work. It could be annoying to rewrite thousand and thousand of structure just to fix this.
//! This is why the next macro was created for.
//! 
//! ### BufferData
//! 
//! BufferData create a ordered structure from a chaotic structure. It come with bytemuck derive macro out of the box.
//! 
//! #### Example
//! ```
//! #[repr(C)]
//! #[derive(wrld::Desc, wrld::BufferData)]
//! struct Vertex {
//!     uv: [f32; 2],
//!     #[f32x2(0)] position: [f32; 2],
//!     data: String,
//!     #[f32x4(1)] color: [f32; 4]
//! }
//! ```
use proc_macro::{TokenStream};

mod converter;
mod parser;
mod macros;

/// Desc is a proc derive macro that allow you to describe a structure as a description to pass to a renderpipeline.
///
/// ## Example
/// ```
/// use wrld::Desc;
///
/// #[repr(C)]
/// #[derive(Desc)]
/// struct Test {
///     #[f32x3(0)] position: Vector3
///     #[f32x4(1)] color: Vector4
/// }
/// ```
/// into
/// ```
/// impl Test {
///     pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
///         // let size_f32 = size_of::<f32>() = 4
///         // let f32x3 = size_f32 * 3 = 12;
///         // let f32x4 = size_f32 * 4 = 16;
///         // let array_stride = 12 + 16 = 28;
/// 
///         wgpu::VertexBufferLayout {
///             array_stride: 28 as wgpu::BufferAddress // array_stride variable,
///             step_mode: wgpu::VertexStepMode::Vertex,
///             attributes: &[
///                 wgpu::VertexAttribute {
///                     offset: 0u64,
///                     format: wgpu::VertexFormat::Float32x3,
///                     shader_location: 0u32,
///                 },
///                 wgpu::VertexAttribute {
///                     offset: 12u64,
///                     format: wgpu::VertexFormat::Float32x4,
///                     shader_location: 1u32,
///                 },
///             ],
///         }
///     }
/// }
/// ```
/// 
/// ## Matrice attributes
/// 
/// Matrices attributes are kind of special, because matrices are the only attributes that can take multiple location.
/// 
/// Matrices need two argument :
/// - The type of the matrice (u8, f32, f64, ect...)
/// - And the starting location
/// 
/// Matrices dimension start from 2x2 to 4x4
/// 
/// ### Example
/// ```
/// #[repr(C)]
/// #[derive(wrld::Desc)]
/// struct Actor {
///     #[mat4x2(u8, 0)] transform: [[f32; 4]; 4]
/// }
/// ``` 
/// Will result to
/// ```
/// impl Actor {
///     pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
///         wgpu::VertexBufferLayout {
///             array_stride: 8u64 as wgpu::BufferAddress,
///             step_mode: wgpu::VertexStepMode::Instance,
///             attributes: &[
///                 wgpu::VertexAttribute {
///                     offset: 0u64,
///                     format: wgpu::VertexFormat::Uint8x2,
///                     shader_location: 0u32,
///                 },
///                 wgpu::VertexAttribute {
///                     offset: 2u64,
///                     format: wgpu::VertexFormat::Uint8x2,
///                     shader_location: 1u32,
///                 },
///                 wgpu::VertexAttribute {
///                     offset: 4u64,
///                     format: wgpu::VertexFormat::Uint8x2,
///                     shader_location: 2u32,
///                 },
///                 wgpu::VertexAttribute {
///                     offset: 6u64,
///                     format: wgpu::VertexFormat::Uint8x2,
///                     shader_location: 3u32,
///                 },
///             ],
///         }
///     }
/// }
/// ```
/// So take care while using it.
/// 
/// Also matrix type handle only wgpu VertexFormat type for row.
/// That does mean that matrix like that.
/// ```
/// #[repr(C)]
/// #[derive(wrld::DescInstance)]
/// struct Vertex {
///     #[mat4x3(u8, 0)] transform: [[f32; 4]; 4]
/// }
/// ```
/// Will throw an error :
/// 
/// "Matrix mat4x3 cannot be use with u8 ! Available matrix are mat4x2 or mat4x4 for u8"
/// 
/// 
/// ## Thing to know
/// - Desc will not handle data transformation
/// - Desc does not handle chaotic structure 
#[proc_macro_derive(Desc, attributes(
    u8x2, u8x4, s8x2, s8x4, un8x2, un8x4, sn8x2, sn8x4,
    u16x2, u16x4, s16x2, s16x4, un16x2, un16x4, sn16x2, sn16x4, f16x2, f16x4,
    f32, f32x2, f32x3, f32x4,
    u32, u32x2, u32x3, u32x4,
    s32, s32x2, s32x3, s32x4,
    f64, f64x2, f64x3, f64x4,
    mat2x2, mat2x3, mat2x4,
    mat3x2, mat3x3, mat3x4,
    mat4x2, mat4x3, mat4x4
))]
pub fn derive_wrld_desc(item: TokenStream) -> TokenStream { 
    macros::derive_wrld_desc(item, wgpu::VertexStepMode::Vertex)
}

/// DescInstance is the same as Desc. The only difference is that it change the step mode to Instance instead of Vertex
///
/// ## Example

/// ```
/// use wrld::DescInstance;
///
/// #[repr(C)]
/// #[derive(DescInstance)]
/// struct Test {
///     #[f32x3(0)] position: Vector3
///     #[f32x4(1)] color: Vector4
/// }
/// ```
/// into
/// ```
/// impl Test {
///     pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
///         // let size_f32 = size_of::<f32>() = 4
///         // let f32x3 = size_f32 * 3 = 12;
///         // let f32x4 = size_f32 * 4 = 16;
///         // let array_stride = 12 + 16 = 28;
/// 
///         wgpu::VertexBufferLayout {
///             array_stride: 28 as wgpu::BufferAddress // array_stride variable,
///             step_mode: wgpu::VertexStepMode::Instance,
///             attributes: &[
///                 wgpu::VertexAttribute {
///                     offset: 0u64,
///                     format: wgpu::VertexFormat::Float32x3,
///                     shader_location: 0u32,
///                 },
///                 wgpu::VertexAttribute {
///                     offset: 12u64,
///                     format: wgpu::VertexFormat::Float32x4,
///                     shader_location: 1u32,
///                 },
///             ],
///         }
///     }
/// }
/// ```
#[proc_macro_derive(DescInstance, attributes(
    u8x2, u8x4, s8x2, s8x4, un8x2, un8x4, sn8x2, sn8x4,
    u16x2, u16x4, s16x2, s16x4, un16x2, un16x4, sn16x2, sn16x4, f16x2, f16x4,
    f32, f32x2, f32x3, f32x4,
    u32, u32x2, u32x3, u32x4,
    s32, s32x2, s32x3, s32x4,
    f64, f64x2, f64x3, f64x4,
    mat2x2, mat2x3, mat2x4,
    mat3x2, mat3x3, mat3x4,
    mat4x2, mat4x3, mat4x4
))]
pub fn derive_wrld_desc_instance(item: TokenStream) -> TokenStream { 
    macros::derive_wrld_desc(item, wgpu::VertexStepMode::Instance)
}

/// A macro to handle any type of chaotic structure.
/// 
/// ## What is a chaotic structure ? And what are the structure different type ?
/// 
/// - Chaotic structure :
/// 
/// structure that have attribute but the fields are not ordered (basically put everywhere and not on the top of the structure)
/// 
/// for example
/// ```
/// #[repr(C)]
/// #[derive(wgpu::Desc)]
/// struct Vertex {
///     some_data: String,
///     #[f32x2(0)] position: [f32; 2],
///     some_other_data: TypeDefinedByUser,
///     #[f32x4(1)] color: [f32; 4]
/// }
/// ```
/// 
/// is a chaotic structure because crates like bytemuck will interpret this structure like this.
/// 
/// ```
/// struct Vertex {
///     some_data: String,
///     position: [f32; 2]
/// }
/// ```
/// 
/// - Ordered structure 
/// 
/// is a structure that does put attribute field on the top of the structure.
/// 
/// for example
/// ```
/// #[repr(C)]
/// #[derive(wgpu::Desc)]
/// struct Vertex {
///     #[f32x2(0)] position: [f32; 2],
///     #[f32x4(1)] color: [f32; 4],
///     some_data: String,
///     some_other_data: TypeDefinedByUser
/// }
/// ```
/// 
/// is a ordered structure and bytemuck will interpret this structure like this.
/// 
/// ```
/// struct Vertex {
///     position: [f32; 2],
///     color: [f32; 4]
/// }
/// ```
/// 
/// before that macro, structure like this (chaotic structure)
/// ```
/// #[repr(C)]
/// #[derive(wgpu::Desc)]
/// struct Vertex {
///     uv: [f32; 2],
///     #[f32x2(0)] position: [f32; 2],
///     data: String,
///     #[f32x4(1)] color: [f32; 4]
/// }
/// ```
/// Where not very well handled by wrld, because bytemuck will not look for attribute data. 
/// Which create undefined behaviour on structure data and will not correspond to what we expect to receive.
/// 
/// A solution to that was to reorder structure data fields (ordered structure)
/// ```
/// #[repr(C)]
/// #[derive(wrld::Desc)]
/// struct Vertex {
///     #[f32x2(0)] position: [f32; 2],
///     #[f32x4(1)] color: [f32; 4],
///     
///     uv: [f32; 4],
///     data: String
/// }
/// ```
/// But now with BufferData this is not a problem anymore.
/// BufferData handle any type of chaotic structure so that does mean that this structure for example
/// ```
/// #[repr(C)]
/// #[derive(wrld::Desc)]
/// struct Vertex {
///     uv: [f32; 4],
///     #[f32x2(0)] position: [f32; 2],
///     data: String,
///     #[f32x4(1)] color: [f32; 4]
/// }
/// ```
/// Is handled via this macro and will have the result of what we expect it from.
/// 
/// ## How it's working ?
/// 
/// BufferData create a ordered structure from a chaotic structure. 
/// It take any array or variable and transform it to is correponding ordered structure
/// it also provide function and trait converter accordingly.
/// 
/// ## Example
/// 
/// Take this structure
/// ```
/// #[repr(C)]
/// #[derive(wrld::Desc, wrld::BufferData)]
/// struct Vertex {
///     texture: SomeTextureType,
///     #[f32x3(0)] position: [f32; 3],
///     message: String,
///     #[f21x3(1)] scale: [f32; 3]
/// }
/// ```
/// 
/// This structure will result in this implementation
/// 
/// ```
/// #[repr(C)]
/// #[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
/// struct VertexBufferData {
///     position: [f32; 3],
///     scale: [f32; 3]
/// }
///
/// impl From<Vertex> for VertexBufferData {
///     fn from(other_data_from_ident_to_into: Vertex) -> Self {
///         Self {
///             position: other_data_from_ident_to_into.position,
///             scale: other_data_from_ident_to_into.scale
///         }
///     }
/// }
///
/// impl From<&'static Vertex> for VertexBufferData {
///     fn from(other_data_from_ident_to_into: &'static Vertex) -> Self {
///         Self {
///            position: other_data_from_ident_to_into.position,
///            scale: other_data_from_ident_to_into.scale
///         }
///     }
/// }
///
/// impl PartialEq<Vertex> for VertexBufferData {
///     fn eq(&self, other_ident_data_boolean_condition: &Vertex) -> bool {
///         position == other_ident_data_boolean_condition.position && scale: other_ident_data_boolean_condition.scale
///     }
/// }
///
/// impl FromIterator<Vertex> for Vec<VertexBufferData> {
///     fn from_iter<T: IntoIterator<Item = Vertex>>(iter: T) -> Self {
///         let mut vec_data_from_ident_from_iterator = Vec::new();
///
///         for c in iter {
///             vec_data_from_ident_from_iterator.push(c.into());
///         }
///
///         vec_data_from_ident_from_iterator
///     }
/// }
///
/// impl FromIterator<&'static Vertex> for Vec<VertexBufferData> {
///     fn from_iter<T: IntoIterator<Item = &'static Vertex>>(iter: T) -> Self {
///         let mut vec_data_from_ident_single_from_iterator : Vec<VertexBufferData> = Vec::new();
///
///         for c in iter {
///             vec_data_from_ident_single_from_iterator.push(c.into());
///         }
///
///         vec_data_from_ident_single_from_iterator
///     }
/// }
///
/// impl VertexBufferData {
///     pub const fn const_into(other_ident_data_to_into_const: &Vertex) -> Self {
///         Self {
///             position: other_ident_data_to_into_const.position,
///             scale: other_ident_data_to_into_const.scale
///         }
///     }
/// }
/// 
/// impl Vertex {
///     pub fn mutate<'a>(other_data_from_ident_to_mutate: &'a Vec<VertexBufferData>) -> &'a [u8] {
///         bytemuck::cast_slice(other_data_from_ident_to_mutate.as_slice())
///     }
///
///     pub fn transmute(other_data_from_ident_to_transmute: &'static [Self]) -> Vec<VertexBufferData> {
///         other_data_from_ident_to_transmute.into_iter().collect::<Vec<VertexBufferData>>() 
///     }
/// }
/// 
/// macro_rules! vertex_const_into {
///     ($data: expr) => {
///         VertexBufferData::const_into(&$data)
///     };
/// }
///
/// macro_rules! mutate_vertex {
///     ($data: expr) => {
///         Vertex::mutate(&Vertex::transmute($data))
///     };
/// }
/// ```
/// Also bytemuck is used for converting structure data to wgpu
/// 
/// ## How to use it ?
/// 
/// When you create any chaotic structure for wrld. Just put wrld::BufferData derive macro at the top
/// 
/// ```
/// #[repr(C)]
/// #[derive(wrld::Desc, wrld::BufferData)]
/// struct Vertex {
///     texture: SomeTextureType,
///     #[f32x3(0)] position: [f32; 3],
///     message: String,
///     #[f21x3(1)] scale: [f32; 3]
/// }
/// ```
/// 
/// ### Single variable conversion.
/// 
/// If you only need to convert a single variable. You can do that.
/// 
/// ```
/// let data : VertexBufferData = Vertex { 
///     texture: SomeTextureType::new(), 
///     position: [0.0, 0.0, 0.0], 
///     message: String::from("something"),
///     scale: [1.0, 1.0, 1.0]
/// }.into()
/// ```
/// 
/// If you however want to convert a constant vertex variable.
/// 
/// ```
/// const data : Vertex = Vertex { 
///     texture: SomeTextureType::new(), 
///     position: [0.0, 0.0, 0.0], 
///     message: String::from("something"),
///     scale: [1.0, 1.0, 1.0]
/// }
/// const vertex_buffer_data = VertexBufferData::const_into(&data);
/// // or
/// const vertex_buffer_data_new = vertex_const_into!(data);
/// ```
/// 
/// ### Array conversion
/// 
/// Array conversion is a little bit more complex. We can't use the .into() because rust will not allow that.
/// This is why you will need to transmute the const array first and then mutate it.
/// 
/// ```
/// const data : [Vertex] = [Vertex { 
///     texture: SomeTextureType::new(), 
///     position: [0.0, 0.0, 0.0], 
///     message: String::from("something"),
///     scale: [1.0, 1.0, 1.0]
/// }, Vertex { 
///     texture: SomeTextureType::new(), 
///     position: [0.0, 1.0, 0.0], 
///     message: String::from("something 2"),
///     scale: [1.0, 1.0, 1.0]
/// }]
/// 
/// fn main() {
///     let arr : &[u8] = Vertex::mutate(&Vertex::transmute(data));
///     // or
///     let arr_new : &[u8] = mutate_vertex!(data);
/// 
///     // With wgpu create_buffer_init
///     let device = wgpu::Device::new()
///     
///     let vertex_buffer = device.create_buffer_init(
///         &wgpu::utils::BufferInitDescriptor {
///             label: Some("Buffer init"),
///             contents: Vertex::mutate(&Vertex::transmute(data)),
///             usage: wgpu::BufferUsages::VERTEX
///     })
/// 
///     // or
/// 
///     let vertex_buffer_new = device.create_buffer_init(
///         &wgpu::utils::BufferInitDescriptor {
///             label: Some("Buffer init"),
///             contents: mutate_vertex!(data),
///             usage: wgpu::BufferUsages::VERTEX
///     })
/// }
/// ```
/// 
/// macro name are formated like this.
/// - struct name will be all lowercase
/// - struct that have uppercase letter in his name are prefix with _ and the letter in question except for the starting letter.
/// 
/// ### Example
/// ```
/// #[repr(C)]
/// #[derive(wrld::Desc, wrld::BufferData)]
/// struct VertexData {
///     #[f32x2(0)] position: [f32; 2]
///     #[f32x4(1)] color: [f32; 4]
/// }
/// 
/// // is equal to
/// 
/// macro_rules! vertex_data_const_into {
///     ($data: expr) => {
///         VertexDataBufferData::const_into(&$data)
///     };
/// }
/// macro_rules! mutate_vertex_data { 
///     ($data: expr) => {
///         VertexData::mutate(&VertexData::transmute($data))
///     }; 
/// }
/// ```
/// 
/// ## Why you have created a another macro instead of putting it in wrld::Desc ?
/// 
/// 1. Prevent wrld to be too much invasive.
/// 2. BufferData is not always needed.
/// 3. BufferData is made to handle chaotic structure and not ordered one. (related to 2.)
/// 
/// There is also know problem about naming const variable the same as the quote generated code variable.
/// There is a simple workaround that is to name const variable all uppercase or just change name of the const variable.
/// However this problem only occurs on const variable
#[proc_macro_derive(BufferData)]
pub fn derive_wrld_buffer_data(item: TokenStream) -> TokenStream {
    macros::derive_wrld_buffer_data(item)
}