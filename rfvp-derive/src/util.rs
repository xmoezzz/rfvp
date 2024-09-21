use darling::FromMeta;
use itertools::Itertools;
use syn::{spanned::Spanned, Attribute};

pub fn parse_opt_attribute<Data: FromMeta>(
    span: &impl Spanned,
    attr_name: &str,
    attrs: &[Attribute],
) -> darling::Result<Option<Data>> {
    let data = attrs
        .iter()
        .filter(|a| a.path().is_ident(attr_name))
        .map(|a| Data::from_meta(&a.meta))
        .at_most_one()
        .map_err(|_| {
            darling::Error::custom(format!("Only one #[{}] attribute is allowed", attr_name))
                .with_span(span)
        })?
        .transpose()?;

    Ok(data)
}

pub fn parse_attribute<Data: FromMeta>(
    span: &impl Spanned,
    attr_name: &str,
    attrs: &[Attribute],
) -> darling::Result<Data> {
    let data = parse_opt_attribute(span, attr_name, attrs)?.ok_or_else(|| {
        darling::Error::custom(format!("Missing #[{}] attribute", attr_name)).with_span(span)
    })?;

    Ok(data)
}
