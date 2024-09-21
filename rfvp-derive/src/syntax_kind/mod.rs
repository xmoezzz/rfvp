//! Auto-generates the SyntaxKind and some other utilities for the asm lexer and parser.

mod input;

use std::str::FromStr;

pub use input::{SyntaxKindInput, SyntaxList};
use proc_macro2::{Ident, TokenStream};
use quote::{quote, quote_spanned};
use syn::LitChar;

use crate::syntax_kind::input::{MappingItem, SyntaxMapping};

fn validate_input(input: &SyntaxKindInput) -> TokenStream {
    let token_count = input.technical.ident_list.len()
        + input.punct.mapping_list.len()
        + input.keywords.mapping_list.len()
        + input.literals.ident_list.len()
        + input.tokens.ident_list.len()
        + input.nodes.ident_list.len();

    let token_count_err = if token_count > 128 {
        return quote! {
            compile_error!("Too many token SyntaxKinds (punct + keywords + literals + tokens + nodes), the maximum is 128");
        };
    } else {
        quote! {}
    };

    quote! {
        #token_count_err
    }
}

fn generate_syntax_kind_enum(input: &SyntaxKindInput) -> TokenStream {
    fn generate_variant(ident: &Ident, doc: &str) -> TokenStream {
        quote_spanned! { ident.span() =>
            #[doc = #doc]
            #ident,
        }
    }

    fn generate_list(list: &SyntaxList, doc: &str) -> TokenStream {
        let mut variants = TokenStream::new();
        for ident in &list.ident_list {
            variants.extend(generate_variant(ident, doc));
        }
        variants
    }

    fn generate_mapping(mapping: &SyntaxMapping, doc_prefix: &str) -> TokenStream {
        let mut variants = TokenStream::new();
        for item in &mapping.mapping_list {
            variants.extend(generate_variant(
                &item.ident,
                &format!("{}{}", doc_prefix, item.content.value()),
            ));
        }
        variants
    }

    let technical = generate_list(&input.technical, "Technical token, only used for parsing");
    let punct = generate_mapping(&input.punct, "Punctuation: ");
    let keywords = generate_mapping(&input.keywords, "Keyword: ");
    let literals = generate_list(&input.literals, "A literal");
    let tokens = generate_list(&input.tokens, "A token");
    let nodes = generate_list(&input.nodes, "A syntax node");

    quote! {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        #[repr(u16)]
        pub enum SyntaxKind {
            #technical
            #punct
            #keywords
            #literals
            #tokens
            #nodes
        }
    }
}

fn generate_is_str_keyword(input: &SyntaxKindInput) -> TokenStream {
    let mut keywords = TokenStream::new();

    for MappingItem { ident, content, .. } in &input.keywords.mapping_list {
        keywords.extend(quote! {
            #content => Some(SyntaxKind::#ident),
        });
    }

    quote! {
        pub(crate) fn from_keyword_str(text: &str) -> Option<Self> {
            match text {
                #keywords
                _ => None,
            }
        }
    }
}

fn generate_inherent_impl(input: &SyntaxKindInput) -> TokenStream {
    let is_str_keyword = generate_is_str_keyword(input);

    quote! {
        impl SyntaxKind {
            #is_str_keyword
        }
    }
}

fn generate_from_u16(input: &SyntaxKindInput) -> TokenStream {
    let kinds = input.iter_kinds().collect::<Vec<_>>();

    quote! {
        impl From<u16> for SyntaxKind {
            fn from(kind: u16) -> Self {
                #(const #kinds: u16 = SyntaxKind::#kinds as u16;)*
                match kind {
                    #(#kinds => Self::#kinds,)*
                    _ => panic!("Invalid SyntaxKind: {}", kind),
                }
            }
        }
    }
}

fn generate_into_u16(_: &SyntaxKindInput) -> TokenStream {
    quote! {
        impl From<SyntaxKind> for u16 {
            fn from(kind: SyntaxKind) -> Self {
                kind as u16
            }
        }
    }
}

fn generate_impl_blocks(input: &SyntaxKindInput) -> TokenStream {
    let inherent_impl = generate_inherent_impl(input);
    let from_u16 = generate_from_u16(input);
    let into_u16 = generate_into_u16(input);

    quote! {
        #inherent_impl
        #from_u16
        #into_u16
    }
}

fn generate_t_macro(input: &SyntaxKindInput) -> TokenStream {
    fn generate_mapping(mapping: &SyntaxMapping) -> TokenStream {
        let mut rules = TokenStream::new();

        for MappingItem { ident, content, .. } in &mapping.mapping_list {
            let matcher = match content.value().as_str() {
                "(" | ")" | "[" | "]" | "{" | "}" => {
                    let c = LitChar::new(content.value().chars().next().unwrap(), content.span());
                    quote!(#c)
                }
                other => TokenStream::from_str(other).expect("Invalid punct"),
            };

            rules.extend(quote! {
                 [#matcher] => {
                    $crate::parser::SyntaxKind::#ident
                };
            });
        }

        rules
    }

    let punct = generate_mapping(&input.punct);
    let keywords = generate_mapping(&input.keywords);

    quote! {
        macro_rules! T {
            #punct
            #keywords
        }
        pub(crate) use T;
    }
}

pub fn impl_syntax_kind(input: SyntaxKindInput) -> TokenStream {
    let errors = validate_input(&input);
    let syntax_kind_enum = generate_syntax_kind_enum(&input);
    let impl_block = generate_impl_blocks(&input);
    let t_macro = generate_t_macro(&input);

    quote! {
        #errors

        #syntax_kind_enum
        #impl_block
        #t_macro
    }
}
