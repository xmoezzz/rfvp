use std::io::Cursor;

use anyhow::Context;
use rfvp_core::format::font::{read_lazy_font, LazyFont};

use crate::asset::Asset;

impl Asset for LazyFont {
    fn load_from_bytes(data: Vec<u8>) -> anyhow::Result<Self> {
        read_lazy_font(&mut Cursor::new(data)).context("Reading font")
    }
}
