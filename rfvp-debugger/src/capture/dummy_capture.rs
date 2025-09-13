use super::StdoutCapture;
use crossbeam_channel::Sender;
use anyhow::{Result, anyhow};

pub struct DummyCapture;

impl StdoutCapture for DummyCapture {
    fn start(_log_tx: Sender<String>) -> Result<Self> {
        Err(anyhow!("Stdout capture not supported on this platform"))
    }
    fn stop(self) -> Result<()> { Ok(()) }
}

