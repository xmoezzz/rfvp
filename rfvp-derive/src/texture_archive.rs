use darling::FromMeta;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use synstructure::Structure;

use crate::{
    sanitization::{LAZY_GPU_TEXTURE, TEXTURE_ARCHIVE, TEXTURE_ARCHIVE_BUILDER},
    util::parse_attribute,
};

#[derive(FromMeta)]
struct TxaFieldMeta {
    name: String,
}

pub fn impl_texture_archive(input: Structure) -> TokenStream {
    let vis = &input.ast().vis;
    if let [var] = input.variants() {
        let builder_fields = var.ast().fields.iter().map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let ty = &f.ty;
            quote! {
                #ident: ::core::option::Option<#ty>
            }
        });
        let builder_new = var.ast().fields.iter().map(|f| {
            let ident = f.ident.as_ref().unwrap();
            quote! {
                #ident: ::core::option::Option::None
            }
        });
        let builder_add_texture = var.ast().fields.iter().map(|f| {
            let ident = f.ident.as_ref().unwrap();

            // TODO: use darling's accumulator pattern
            let meta = parse_attribute::<TxaFieldMeta>(&f, "txa", &f.attrs).unwrap();

            let name = meta.name;

            quote! {
                #name => {
                    if self.#ident.is_some() {
                        panic!("Texture {} already added", #name);
                    }
                    self.#ident = ::core::option::Option::Some(texture);
                }
            }
        });
        let builder_result = var.ast().fields.iter().map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let missing_field_error = format!("Missing field: {}", ident);
            quote! {
                #ident: self.#ident.expect(#missing_field_error)
            }
        });
        let ident = &var.ast().ident;
        let builder_ident = Ident::new(&format!("{}Builder", input.ast().ident), Span::call_site());

        let texture_archive = &TEXTURE_ARCHIVE;
        let texture_archive_builder = &TEXTURE_ARCHIVE_BUILDER;
        let lazy_gpu_texture = &LAZY_GPU_TEXTURE;

        quote! {
            #vis struct #builder_ident {
                #(#builder_fields,)*
            }

            impl #texture_archive_builder for #builder_ident {
                type Output = #ident;

                fn new() -> Self {
                    Self {
                        #(#builder_new,)*
                    }
                }
                fn add_texture(&mut self, name: &str, texture: #lazy_gpu_texture) {
                    match name {
                        #(#builder_add_texture)*
                        _ => panic!("Unknown texture: {}", name),
                    }
                }
                fn build(self) -> Self::Output {
                    Self::Output {
                        #(#builder_result,)*
                    }
                }
            }

            impl #texture_archive for #ident {
                type Builder = #builder_ident;
            }
        }
    } else {
        panic!("TextureArchive can only be derived for enums with a single variant")
    }
}
