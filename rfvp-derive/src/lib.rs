// this is noisy & not well-supported by IDEs
#![allow(clippy::uninlined_format_args)]

mod command;
mod rational;
pub(crate) mod sanitization;
mod syntax_kind;
mod texture_archive;
mod util;
mod vertex;

use proc_macro::TokenStream;
use synstructure::macros::DeriveInput;

use crate::{
    syntax_kind::{impl_syntax_kind, SyntaxKindInput},
    vertex::impl_vertex,
};

/// A WIP replacement for the wrld macro.
#[proc_macro_derive(
    Vertex,
    attributes(
        u8x2, u8x4, s8x2, s8x4, un8x2, un8x4, sn8x2, sn8x4, u16x2, u16x4, s16x2, s16x4, un16x2,
        un16x4, sn16x2, sn16x4, f16x2, f16x4, f32, f32x2, f32x3, f32x4, u32, u32x2, u32x3, u32x4,
        s32, s32x2, s32x3, s32x4, f64, f64x2, f64x3, f64x4
    )
)]
pub fn derive_vertex(input: TokenStream) -> TokenStream {
    match synstructure::macros::parse::<DeriveInput>(input) {
        Ok(p) => match synstructure::Structure::try_new(&p) {
            Ok(s) => synstructure::MacroResult::into_stream(impl_vertex(s)),
            Err(e) => e.to_compile_error().into(),
        },
        Err(e) => e.to_compile_error().into(),
    }
}

/// Generates a `SyntaxKind` enum, and some associated impls. For use in `shin-asm`.
#[proc_macro]
pub fn syntax_kind(input: TokenStream) -> TokenStream {
    match syn::parse::<SyntaxKindInput>(input) {
        Ok(p) => synstructure::MacroResult::into_stream(impl_syntax_kind(p)),
        Err(e) => e.to_compile_error().into(),
    }
}


/// Creates a `Rational` literal
#[proc_macro]
pub fn rat(input: TokenStream) -> TokenStream {
    match syn::parse::<syn::Lit>(input) {
        Ok(p) => rational::impl_rational(p).into(),
        Err(e) => e.to_compile_error().into(),
    }
}
