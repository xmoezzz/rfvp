use crossbeam_channel::Sender;
use anyhow::Result;

pub trait StdoutCapture {
    fn start(log_tx: Sender<String>) -> Result<Self> where Self: Sized;
    fn stop(self) -> Result<()>;
}

#[cfg(unix)]
mod unix_capture;
#[cfg(unix)]
pub use unix_capture::UnixCapture as PlatformCapture;

#[cfg(windows)]
mod windows_capture;
#[cfg(windows)]
pub use windows_capture::WinCapture as PlatformCapture;

#[cfg(not(any(unix, windows)))]
mod dummy_capture;
#[cfg(not(any(unix, windows)))]
pub use dummy_capture::DummyCapture as PlatformCapture;
