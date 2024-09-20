use anyhow::{Context, Result};
use rfvp_core::format::audio::{read_audio, AudioFile};

use crate::asset::Asset;

impl Asset for AudioFile {
    fn load_from_bytes(data: Vec<u8>) -> Result<Self> {
        read_audio(&data).context("Parsing audio file")
    }
}
