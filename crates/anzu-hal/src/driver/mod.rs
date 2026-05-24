//! Audio hardware driver trait and platform dispatch.

pub mod null;

/// Intel HDA driver — x86_64 (CF8/CFC PCI + MMIO) and aarch64 (ECAM PCI + MMIO).
pub mod hda;

/// Intel AC97 driver — x86_64 only (I/O-port PCI + I/O-port registers).
#[cfg(all(feature = "ac97", target_arch = "x86_64"))]
pub mod ac97;

pub use hda::set_ecam_base;

/// Hardware audio output driver.
pub trait AudioDriver: Send {
    fn is_available(&self) -> bool;
    /// Number of 16-bit stereo frames the hardware can accept right now.
    fn frames_available(&self) -> usize;
    /// Write `n_frames` stereo 16-bit LE frames from `buf` to the hardware.
    fn write_frames(&mut self, buf: &[i16], n_frames: usize);
}

/// Select the best available driver for the current target.
pub fn create_driver() -> Box<dyn AudioDriver> {
    // Try Intel HDA first (works on x86_64 QEMU q35 and all aarch64 QEMU virt).
    {
        let drv = hda::HdaDriver::new();
        if drv.is_available() {
            log::info!("anzu-hal: using Intel HDA audio driver");
            return Box::new(drv);
        }
        log::info!("anzu-hal: Intel HDA not found");
    }

    // Fall back to AC97 on x86_64 (older QEMU i440fx machine type).
    #[cfg(all(feature = "ac97", target_arch = "x86_64"))]
    {
        let drv = ac97::Ac97Driver::new();
        if drv.is_available() {
            log::info!("anzu-hal: using AC97 audio driver");
            return Box::new(drv);
        }
        log::warn!("anzu-hal: AC97 not found either, falling back to null driver");
    }

    log::info!("anzu-hal: using null (silent) audio driver");
    Box::new(null::NullDriver)
}
