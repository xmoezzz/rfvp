use proc_macro2::{Ident, Span};
use syn::{
    braced, bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token, LitStr, Token,
};

#[derive(Debug)]
pub struct SyntaxList {
    pub bracket_token: token::Bracket,
    pub ident_list: Punctuated<Ident, Token![,]>,
}

impl SyntaxList {
    fn span(&self) -> Span {
        self.bracket_token.span.span()
    }
    pub fn iter_idents(&self) -> impl Iterator<Item = &Ident> {
        self.ident_list.iter()
    }
}

impl Parse for SyntaxList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        Ok(Self {
            bracket_token: bracketed!(content in input),
            ident_list: content.parse_terminated(Ident::parse, Token![,])?,
        })
    }
}

#[derive(Debug)]
pub struct MappingItem {
    pub ident: Ident,
    pub fat_arrow_token: Token![=>],
    pub content: LitStr,
}

impl Parse for MappingItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = input.parse()?;
        let fat_arrow_token = input.parse()?;
        let content = input.parse()?;
        Ok(Self {
            ident,
            fat_arrow_token,
            content,
        })
    }
}

#[derive(Debug)]
pub struct SyntaxMapping {
    pub brace_token: token::Brace,
    pub mapping_list: Punctuated<MappingItem, Token![,]>,
}

impl SyntaxMapping {
    fn span(&self) -> Span {
        self.brace_token.span.span()
    }
    pub fn iter_idents(&self) -> impl Iterator<Item = &Ident> {
        self.mapping_list.iter().map(|v| &v.ident)
    }
}

impl Parse for SyntaxMapping {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        Ok(Self {
            brace_token: braced!(content in input),
            mapping_list: content.parse_terminated(MappingItem::parse, Token![,])?,
        })
    }
}

#[derive(Debug)]
enum SyntaxKindContents {
    Mapping(SyntaxMapping),
    List(SyntaxList),
}

impl SyntaxKindContents {
    pub fn into_mapping(self) -> syn::Result<SyntaxMapping> {
        match self {
            Self::Mapping(mapping) => Ok(mapping),
            Self::List(list) => Err(syn::Error::new(
                list.span(),
                "Expected a mapping (denoted by {})",
            )),
        }
    }

    pub fn into_list(self) -> syn::Result<SyntaxList> {
        match self {
            Self::Mapping(mapping) => Err(syn::Error::new(
                mapping.span(),
                "Expected a list (denoted by [])",
            )),
            Self::List(list) => Ok(list),
        }
    }
}

impl Parse for SyntaxKindContents {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(token::Brace) {
            let mapping = input.parse()?;
            Ok(Self::Mapping(mapping))
        } else if input.peek(token::Bracket) {
            let list = input.parse()?;
            Ok(Self::List(list))
        } else {
            Err(syn::Error::new(
                input.span(),
                "Expected a mapping or a list",
            ))
        }
    }
}

#[derive(Debug)]
enum SyntaxKindIdent {
    Technical(Span),
    Punct(Span),
    Keyword(Span),
    Literal(Span),
    Token(Span),
    Node(Span),
}

impl Parse for SyntaxKindIdent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        let span = ident.span();
        match ident.to_string().as_str() {
            "technical" => Ok(Self::Technical(span)),
            "punct" => Ok(Self::Punct(span)),
            "keywords" => Ok(Self::Keyword(span)),
            "literals" => Ok(Self::Literal(span)),
            "tokens" => Ok(Self::Token(span)),
            "nodes" => Ok(Self::Node(span)),
            _ => Err(syn::Error::new(
                ident.span(),
                "Expected one of punct, literals, tokens, nodes",
            )),
        }
    }
}

#[derive(Debug)]
struct SyntaxKindItem {
    ident: SyntaxKindIdent,
    #[allow(dead_code)]
    colon_token: Token![:],
    content: SyntaxKindContents,
}

impl Parse for SyntaxKindItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = input.parse()?;
        let colon_token = input.parse()?;
        let content = input.parse()?;
        Ok(Self {
            ident,
            colon_token,
            content,
        })
    }
}

// technical: [
//   EOF,
//   TOMBSTONE,
// ],
// punct: {
//   EQ => "=",
//   EQ2 => "==",
// },
// keywords: {
//   MOD => "mod",
// },
// literals: [
//   INT_NUMBER,
//   RATIONAL_NUMBER,
//   STRING,
// ],
// tokens: [
//   ERROR,
//   IDENT,
//   WHITESPACE,
//   COMMENT,
// ],
// nodes: [
//   SOURCE_FILE,
// ],
#[derive(Debug)]
pub struct SyntaxKindInput {
    pub technical: SyntaxList,
    pub punct: SyntaxMapping,
    pub keywords: SyntaxMapping,
    pub literals: SyntaxList,
    pub tokens: SyntaxList,
    pub nodes: SyntaxList,
}

impl SyntaxKindInput {
    pub fn iter_kinds(&self) -> impl Iterator<Item = &Ident> + '_ {
        self.technical
            .iter_idents()
            .chain(self.punct.iter_idents())
            .chain(self.keywords.iter_idents())
            .chain(self.literals.iter_idents())
            .chain(self.tokens.iter_idents())
            .chain(self.nodes.iter_idents())
    }
}

impl Parse for SyntaxKindInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let kind_list = input.parse_terminated(SyntaxKindItem::parse, Token![,])?;

        let mut technical = None;
        let mut punct = None;
        let mut keywords = None;
        let mut literals = None;
        let mut tokens = None;
        let mut nodes = None;

        for item in kind_list {
            match item.ident {
                SyntaxKindIdent::Technical(span) => {
                    if technical.is_some() {
                        return Err(syn::Error::new(span, "Technical can only be defined once"));
                    }
                    technical = Some(item.content.into_list()?);
                }
                SyntaxKindIdent::Punct(span) => {
                    if punct.is_some() {
                        return Err(syn::Error::new(span, "Punct can only be defined once"));
                    }
                    punct = Some(item.content.into_mapping()?);
                }
                SyntaxKindIdent::Keyword(span) => {
                    if tokens.is_some() {
                        return Err(syn::Error::new(span, "Tokens can only be defined once"));
                    }
                    keywords = Some(item.content.into_mapping()?);
                }
                SyntaxKindIdent::Literal(span) => {
                    if literals.is_some() {
                        return Err(syn::Error::new(span, "Literals can only be defined once"));
                    }
                    literals = Some(item.content.into_list()?);
                }
                SyntaxKindIdent::Token(span) => {
                    if tokens.is_some() {
                        return Err(syn::Error::new(span, "Tokens can only be defined once"));
                    }
                    tokens = Some(item.content.into_list()?);
                }
                SyntaxKindIdent::Node(span) => {
                    if nodes.is_some() {
                        return Err(syn::Error::new(span, "Nodes can only be defined once"));
                    }
                    nodes = Some(item.content.into_list()?);
                }
            }
        }

        Ok(Self {
            technical: technical
                .ok_or_else(|| syn::Error::new(input.span(), "Technical must be defined"))?,
            punct: punct.ok_or_else(|| syn::Error::new(input.span(), "Punct must be defined"))?,
            keywords: keywords
                .ok_or_else(|| syn::Error::new(input.span(), "Keywords must be defined"))?,
            literals: literals
                .ok_or_else(|| syn::Error::new(input.span(), "Literals must be defined"))?,
            tokens: tokens
                .ok_or_else(|| syn::Error::new(input.span(), "Tokens must be defined"))?,
            nodes: nodes.ok_or_else(|| syn::Error::new(input.span(), "Nodes must be defined"))?,
        })
    }
}
