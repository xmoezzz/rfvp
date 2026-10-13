use clap::Parser;

fn main() -> anyhow::Result<()> {
    rfvp_psx::convert(
        &rfvp_psx::ConversionOptions::parse(),
        &rfvp_converter_api::ConversionContext::human(),
    )
}
