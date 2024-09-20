mod converter;
mod wgpu;

use std::{fmt, fmt::Display};

use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Data, DataStruct, DeriveInput, Meta};
use synstructure::Structure;

use crate::vertex::{converter::convert_type_to_wgpu, wgpu::VertexStepMode};
// The code here is very much based on https://github.com/CorentinDeblock/wrld
// wrld has stopped being updated, and the crates.io version doesn't compile on windows anymore due to a dependency on an old version of wgpu.
// I've wanted to do some tweaks to it anyway (quoting past me: "I want to have my own traits for this anyways"), so we are vendoring the code.

#[derive(Debug)]
struct Entity {
    fields: Vec<EntityFields>,
}

#[derive(Debug)]
struct EntityFieldsAttrs {
    name: String,
    data: u32,
}

#[derive(Debug)]
struct EntityFields {
    attrs: Vec<EntityFieldsAttrs>,
    name: proc_macro2::Ident,
    ty: syn::Type,
}

struct DisplayAttrStyle<'a>(pub &'a syn::AttrStyle);

impl<'a> Display for DisplayAttrStyle<'a> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self.0 {
            syn::AttrStyle::Outer => "#",
            syn::AttrStyle::Inner(_) => "#!",
        })
    }
}

struct DisplayPath<'a>(pub &'a syn::Path);

impl<'a> Display for DisplayPath<'a> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        for (i, segment) in self.0.segments.iter().enumerate() {
            if i > 0 || self.0.leading_colon.is_some() {
                formatter.write_str("::")?;
            }
            write!(formatter, "{}", segment.ident)?;
        }
        Ok(())
    }
}

fn get_entity_field(field: &syn::Field) -> syn::Result<EntityFields> {
    let mut attrs: Vec<EntityFieldsAttrs> = Vec::new();

    for attr in &field.attrs {
        match &attr.meta {
            Meta::Path(path) => {
                return Err(syn::Error::new(
                    path.segments.span(),
                    format!(
                        "expected attribute arguments in parentheses: {}[{}(...)]",
                        DisplayAttrStyle(&attr.style),
                        DisplayPath(path),
                    ),
                ))
            }
            Meta::NameValue(meta) => {
                return Err(syn::Error::new(
                    meta.eq_token.span,
                    format_args!(
                        "expected parentheses: {}[{}(...)]",
                        DisplayAttrStyle(&attr.style),
                        DisplayPath(&meta.path),
                    ),
                ))
            }
            Meta::List(meta) => {
                // TODO: ignore attributes that do not belong to use (or does the compiler already filter those for us?
                let lint: syn::LitInt = meta
                    .parse_args()
                    .expect("Only integer is authorize for shader location data");

                attrs.push(EntityFieldsAttrs {
                    name: meta.path.require_ident()?.to_string(),
                    data: lint.base10_parse().unwrap(),
                });
            }
        }
    }

    let entity_fields = EntityFields {
        attrs,
        name: field.ident.clone().unwrap(),
        ty: field.ty.clone(),
    };

    Ok(entity_fields)
}

fn process_wgpu_type(
    format: &converter::WGPUData,
    shader_locations: &mut Vec<u32>,
    attrs: &mut Vec<proc_macro2::TokenStream>,
    offset: &u64,
) {
    let tty = format.wgpu_type.ty;
    let shader_location = format.shader_location;

    if shader_locations.contains(&shader_location) {
        panic!("Cannot have two time the same location in the same struct");
    }

    shader_locations.push(shader_location);

    attrs.push(quote::quote! {
        wgpu::VertexAttribute {
            offset: #offset,
            format: #tty,
            shader_location: #shader_location
        }
    });
}

// TODO: implement vertex macro
// it would be a replacement for sometimes clunky wrld
pub fn impl_vertex(input: Structure) -> TokenStream {
    let DeriveInput {
        attrs: _,
        vis: _,
        ident,
        generics,
        data: Data::Struct(DataStruct { fields, .. }),
    } = &input.ast()
    else {
        let e = syn::Error::new(
            input.ast().span(),
            "Only struct is allowed to be used in derive(Vertex)",
        )
        .to_compile_error();
        return quote! {
            #e
        };
    };

    let entity = Entity {
        fields: match fields
            .iter()
            .map(get_entity_field)
            .collect::<syn::Result<_>>()
        {
            Ok(r) => r,
            Err(e) => {
                let e = e.to_compile_error();
                return quote! {
                    #e
                };
            }
        },
    };

    let mut attrs: Vec<TokenStream> = Vec::new();

    let mut offset: u64 = 0;
    let mut shader_locations: Vec<u32> = Vec::new();

    for i in entity.fields {
        for attr in i.attrs {
            let format = convert_type_to_wgpu(&attr.name, attr.data).unwrap();
            process_wgpu_type(&format, &mut shader_locations, &mut attrs, &offset);
            offset += format.wgpu_type.offset;
        }
    }

    let step_mode = VertexStepMode::Vertex;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote::quote! {
        impl #impl_generics #ident #ty_generics #where_clause {
            pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
                wgpu::VertexBufferLayout {
                    array_stride: #offset as wgpu::BufferAddress,
                    step_mode: #step_mode,
                    attributes: &[#(#attrs),*]
                }
            }
        }
    }
}
