#[cfg(target_arch = "wasm32")]
pub use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "uefi")))]
pub use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "uefi")]
mod uefi_time {
    use core::sync::atomic::{AtomicU64, Ordering};
    use std::ops::Sub;
    pub use std::time::Duration;

    // ── Arch-specific monotonic microsecond counter ───────────────────────────
    //
    // x86_64: RDTSC calibrated by a one-time 10 ms boot::stall.
    // aarch64: CNTPCT_EL0 / CNTFRQ_EL0 (physical counter; always accessible
    //          from EL2 where UEFI firmware runs).
    // fallback: EFI_RUNTIME_SERVICES::GetTime (1 s precision if nanoseconds
    //           are not implemented by the firmware — typically on QEMU/OVMF).

    // ── x86_64 TSC state ─────────────────────────────────────────────────────
    #[cfg(target_arch = "x86_64")]
    static X86_TSC_TICKS_PER_US: AtomicU64 = AtomicU64::new(0);
    #[cfg(target_arch = "x86_64")]
    static X86_TSC_ORIGIN: AtomicU64 = AtomicU64::new(u64::MAX);

    #[cfg(target_arch = "x86_64")]
    fn x86_now_us() -> Option<u64> {
        let origin = X86_TSC_ORIGIN.load(Ordering::Relaxed);
        if origin == u64::MAX {
            return None;
        }
        let tpu = X86_TSC_TICKS_PER_US.load(Ordering::Relaxed);
        if tpu == 0 {
            return None;
        }
        let tsc = unsafe { core::arch::x86_64::_rdtsc() };
        Some(tsc.saturating_sub(origin) / tpu)
    }

    // ── aarch64 counter state ─────────────────────────────────────────────────
    #[cfg(target_arch = "aarch64")]
    static AARCH64_FREQ: AtomicU64 = AtomicU64::new(0);
    #[cfg(target_arch = "aarch64")]
    static AARCH64_ORIGIN: AtomicU64 = AtomicU64::new(u64::MAX);

    #[cfg(target_arch = "aarch64")]
    fn aarch64_now_us() -> Option<u64> {
        let origin = AARCH64_ORIGIN.load(Ordering::Relaxed);
        if origin == u64::MAX {
            return None;
        }
        let freq = AARCH64_FREQ.load(Ordering::Relaxed);
        if freq == 0 {
            return None;
        }
        let cnt: u64;
        // SAFETY: CNTPCT_EL0 (physical counter) is always readable from EL2
        // where UEFI runs on AArch64. CNTFRQ_EL0 gives its frequency in Hz.
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) cnt, options(nomem, nostack));
        }
        // Saturating subtraction guards against counter wrap on very long runs.
        let elapsed = cnt.saturating_sub(origin);
        // Use u128 to avoid overflow for large elapsed * 1_000_000 products.
        Some((elapsed as u128 * 1_000_000 / freq as u128) as u64)
    }

    // ── UEFI GetTime fallback ─────────────────────────────────────────────────
    static FALLBACK_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn gettime_us() -> u64 {
        match uefi::runtime::get_time() {
            Ok(t) => {
                let h = t.hour() as u64;
                let m = t.minute() as u64;
                let s = t.second() as u64;
                let ns = t.nanosecond() as u64;
                (h * 3_600_000 + m * 60_000 + s * 1_000 + ns / 1_000_000) * 1_000
                    + (ns % 1_000_000) / 1_000
            }
            Err(_) => {
                // GetTime unavailable: advance by 1 ms per call as a last resort.
                FALLBACK_COUNTER.fetch_add(1_000, Ordering::Relaxed)
            }
        }
    }

    fn now_us() -> u64 {
        #[cfg(target_arch = "x86_64")]
        if let Some(us) = x86_now_us() {
            return us;
        }
        #[cfg(target_arch = "aarch64")]
        if let Some(us) = aarch64_now_us() {
            return us;
        }
        gettime_us()
    }

    // ── One-time calibration ──────────────────────────────────────────────────
    //
    // Call `calibrate()` once at UEFI startup (before any `Instant::now()`
    // calls) so the fast arch-specific path is enabled.

    #[cfg(target_arch = "x86_64")]
    pub fn calibrate() {
        let t0 = unsafe { core::arch::x86_64::_rdtsc() };
        X86_TSC_ORIGIN.store(t0, Ordering::Relaxed);
        // Stall 10 ms so we can measure TSC ticks per microsecond.
        uefi::boot::stall(core::time::Duration::from_millis(10));
        let t1 = unsafe { core::arch::x86_64::_rdtsc() };
        let delta_ticks = t1.saturating_sub(t0).max(1);
        // 10 ms = 10_000 µs → ticks per µs = delta / 10_000
        let tpu = (delta_ticks / 10_000).max(1);
        X86_TSC_TICKS_PER_US.store(tpu, Ordering::Relaxed);
    }

    #[cfg(target_arch = "aarch64")]
    pub fn calibrate() {
        let cnt: u64;
        let freq: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) cnt, options(nomem, nostack));
            core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq, options(nomem, nostack));
        }
        AARCH64_FREQ.store(freq.max(1), Ordering::Relaxed);
        AARCH64_ORIGIN.store(cnt, Ordering::Relaxed);
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    pub fn calibrate() {}

    // ── Public time types ─────────────────────────────────────────────────────

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Instant {
        us: u64,
    }

    impl Instant {
        pub fn now() -> Self {
            Self { us: now_us() }
        }

        pub fn elapsed(&self) -> Duration {
            Duration::from_micros(now_us().saturating_sub(self.us))
        }
    }

    impl Sub for Instant {
        type Output = Duration;

        fn sub(self, rhs: Self) -> Self::Output {
            Duration::from_micros(self.us.saturating_sub(rhs.us))
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct SystemTime {
        us: u64,
    }

    pub const UNIX_EPOCH: SystemTime = SystemTime { us: 0 };

    impl SystemTime {
        pub fn now() -> Self {
            Self { us: now_us() }
        }

        pub fn duration_since(&self, earlier: SystemTime) -> Result<Duration, Duration> {
            self.us
                .checked_sub(earlier.us)
                .map(Duration::from_micros)
                .ok_or_else(|| Duration::from_micros(earlier.us - self.us))
        }
    }
}

#[cfg(target_os = "uefi")]
pub use uefi_time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Re-export calibrate() for UEFI startup use.
#[cfg(target_os = "uefi")]
pub use uefi_time::calibrate as calibrate_uefi_clock;
