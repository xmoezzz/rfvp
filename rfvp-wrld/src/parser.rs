use proc_macro2::{Ident, Punct, Span, Spacing};
use quote::TokenStreamExt;

pub struct TokenVertexFormat {
    pub attribute: wgpu::VertexFormat
}

impl quote::ToTokens for TokenVertexFormat {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        //tokens.append_separated(&["wgpu", "VertexFormat", "Float32x2"], "::");
        tokens.append(Ident::new("wgpu", Span::call_site()));
        tokens.append(Punct::new(':', Spacing::Joint));
        tokens.append(Punct::new(':', Spacing::Alone));
        tokens.append(Ident::new("VertexFormat", Span::call_site()));
        tokens.append(Punct::new(':', Spacing::Joint));
        tokens.append(Punct::new(':', Spacing::Alone));
        tokens.append(Ident::new(format!("{:?}", self.attribute).as_str(), Span::call_site()));
    }
}

pub struct TokenVertexStepMode {
    pub step_mode: wgpu::VertexStepMode
}

impl quote::ToTokens for TokenVertexStepMode {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        //tokens.append_separated(&["wgpu", "VertexFormat", "Float32x2"], "::");
        tokens.append(Ident::new("wgpu", Span::call_site()));
        tokens.append(Punct::new(':', Spacing::Joint));
        tokens.append(Punct::new(':', Spacing::Alone));
        tokens.append(Ident::new("VertexStepMode", Span::call_site()));
        tokens.append(Punct::new(':', Spacing::Joint));
        tokens.append(Punct::new(':', Spacing::Alone));
        tokens.append(Ident::new(format!("{:?}", self.step_mode).as_str(), Span::call_site()));
    }
}

pub struct AttrData<'a> {
    pub attribute: &'a syn::Attribute,
    pub segment: &'a syn::PathSegment
}

pub fn parse_attrs<'a>(attrs : &'a std::vec::Vec<syn::Attribute>, mut callback: Box<dyn FnMut(AttrData) + 'a>) {
    attrs.iter().for_each(|a| {
        a.path.segments.iter().for_each(|ps| {
            callback(AttrData {
                attribute: &a,
                segment: &ps,
            });
        });
    });
}

#[derive(Debug, Clone)]
pub struct AttrMat {
    pub ident: syn::Ident,
    pub data: u32
}

impl syn::parse::Parse for AttrMat {
    fn parse(tokens: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident : syn::Ident = tokens.parse()?;
        tokens.parse::<syn::Token![,]>()?;
        let data : syn::LitInt = tokens.parse()?;

        Ok(AttrMat { ident, data: data.base10_parse().unwrap() })
    }
}