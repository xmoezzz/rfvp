use clap::Parser;

fn main() -> anyhow::Result<()> {
    rfvp_rebuilder::convert(
        &rfvp_rebuilder::ConversionOptions::parse(),
        &rfvp_converter_api::ConversionContext::human(),
    )
}
