//! ARM support and utils

use core::arch::asm;

/// Represents a CPU register value (`W`, `X` or `R` value depending on the context/arch).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct CpuRegister {
    /// The register value.
    pub reg: u64,
}

impl CpuRegister {
    /// Gets the [`CpuRegister`] as an `X` value.
    #[inline]
    pub const fn get_x(&self) -> u64 {
        self.reg
    }

    /// Sets the [`CpuRegister`] from an `X` value.
    ///
    /// # Arguments:
    ///
    /// * `x`: The value to set.
    #[inline]
    pub fn set_x(&mut self, x: u64) {
        self.reg = x;
    }

    /// Gets the [`CpuRegister`] as an `W` value.
    #[inline]
    pub const fn get_w(&self) -> u32 {
        self.reg as u32
    }

    /// Sets the [`CpuRegister`] from an `W` value.
    ///
    /// # Arguments:
    ///
    /// * `w`: The value to set.
    #[inline]
    pub fn set_w(&mut self, w: u32) {
        self.reg = w as u64;
    }

    /// Gets the [`CpuRegister`] as an `R` value.
    #[inline]
    pub const fn get_r(&self) -> u32 {
        self.reg as u32
    }

    /// Sets the [`CpuRegister`] from an `R` value.
    ///
    /// # Arguments:
    ///
    /// * `r`: The value to set.
    #[inline]
    pub fn set_r(&mut self, r: u32) {
        self.reg = r as u64;
    }
}

/// Represents a FPU register value (`V`, `D` or `S` value depending on the context/arch).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct FpuRegister {
    /// The register value.
    pub reg: u128,
}

impl FpuRegister {
    /// Gets the [`FpuRegister`] as an `V` value.
    #[inline]
    pub const fn get_v(&self) -> u128 {
        self.reg
    }

    /// Sets the [`FpuRegister`] from an `V` value.
    ///
    /// # Arguments:
    ///
    /// * `v`: The value to set.
    #[inline]
    pub fn set_v(&mut self, v: u128) {
        self.reg = v;
    }

    /// Gets the [`FpuRegister`] as an `D` value.
    #[inline]
    pub const fn get_d(&self) -> f64 {
        self.reg as f64
    }

    /// Sets the [`FpuRegister`] from an `D` value.
    ///
    /// # Arguments:
    ///
    /// * `d`: The value to set.
    #[inline]
    pub fn set_d(&mut self, d: f64) {
        self.reg = d.to_bits() as u128;
    }

    /// Gets the [`FpuRegister`] as an `S` value.
    #[inline]
    pub const fn get_s(&self) -> f32 {
        f32::from_bits(self.reg as u32)
    }

    /// Sets the [`FpuRegister`] from an `S` value.
    ///
    /// # Arguments:
    ///
    /// * `s`: The value to set.
    #[inline]
    pub fn set_s(&mut self, s: f32) {
        self.reg = s.to_bits() as u128;
    }
}

define_bit_set! {
    /// Represents flags of different register kinds/groups.
    RegisterGroup (u32) {
        CpuGprs = bit!(0),
        CpuSprs = bit!(1),
        FpuGprs = bit!(2),
        FpuSprs = bit!(3)
    }
}

/// Represents a thread context usable with [`svc`][`crate::svc`]s.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ThreadContext {
    /// The general-purpose CPU registers.
    pub gpu_gprs: [CpuRegister; 29],
    /// The FP register.
    pub fp: u64,
    /// The LR register.
    pub lr: u64,
    /// The SP register.
    pub sp: u64,
    /// The PC register.
    pub pc: CpuRegister,
    /// The PSR value.
    pub psr: u32,
    /// The general-purpose FPU registers.
    pub fpu_gprs: [FpuRegister; 32],
    /// The FPCR value.
    pub fpcr: u32,
    /// The FPSR value.
    pub fpsr: u32,
    /// The TPIDR value.
    pub tpidr: u64,
}

/// Flushes (clean + invalidate) memory cache at a certain memory location.
///
/// The start and end address are rounded to cache line boundaries read from the `CTR_EL0` register.
///
/// # Arguments:
///
/// * `address`: Memory address.
/// * `size`: Memory size.
#[inline(always)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn cache_flush(address: *mut u8, size: usize) {
    // Equivalent to `cache_flush2` commented out below, but ends up being better hand-written
    // than compiler optimised.
    #[unsafe(naked)]
    unsafe extern "C" fn __nx_arm_cache_flush(address: *mut u8, size: usize) {
        core::arch::naked_asm!(
            crate::macros::util::maybe_cfi!(".cfi_startproc"),
            "add x1, x1, x0",
            "mrs x8, CTR_EL0",
            "lsr x8, x8, #16",
            "and x8, x8, #0xf",
            "mov x9, #4",
            "lsl x9, x9, x8",
            "sub x10, x9, #1",
            "bic x8, x0, x10",
            "mov x10, x1",
            "mov w1, #1",
            "mrs x0, tpidrro_el0",
            "strb w1, [x0, #0x104] ", // Set flag at TLR[0x104] for kernel
            "2:",
            "dc  civac, x8",
            "add x8, x8, x9",
            "cmp x8, x10",
            "bcc 2b",
            "dsb sy",
            "strb wzr, [x0, #0x104]", // Unset flag at TLR[0x104] for kernel
            "ret",
            crate::macros::util::maybe_cfi!(".cfi_endproc")
        );
    }

    unsafe {
        __nx_arm_cache_flush(address, size);
    }
}

/*
pub fn cache_flush2(address: *mut u8, size: usize) {
    let address = address.expose_provenance();
    let mut ctr_el0: u64;
    unsafe {
        asm!("mrs {}, CTR_EL0", out(reg) ctr_el0);
    }

    let cache_line_size = 4usize << (ctr_el0 as usize >> 16 & 0xF);
    let cache_line_mask = !(cache_line_size - 1);
    let last_address = address.saturating_add(size) & cache_line_mask;
    let mut address = address & cache_line_mask;

    unsafe {
        let tlr = nx::thread::get_thread_local_region();
        (*tlr).cache_maintenance_flag = true;
        while address <= last_address {
            asm!("dc civac, {}", in(reg) address);
            address = address.saturating_add(cache_line_size);
        }

        asm!("dsb sy");

        (*tlr).cache_maintenance_flag = false;
    }
}
*/

/// Gets the system tick.
#[inline(always)]
pub fn get_system_tick() -> u64 {
    let system_tick: u64;
    unsafe {
        asm!(
            "mrs {}, cntpct_el0",
            out(reg) system_tick
        );
    }
    system_tick
}

/// Gets the system tick time as nanoseconds.
#[inline(always)]
pub fn get_system_tick_as_nanos() -> u64 {
    get_system_tick() / (get_system_tick_frequency() / 1_000_000_000u64)
}

/// Gets the system tick frequency.
#[inline(always)]
pub fn get_system_tick_frequency() -> u64 {
    let system_tick_freq: u64;
    unsafe {
        asm!(
            "mrs {}, cntfrq_el0",
            out(reg) system_tick_freq
        );
    }
    system_tick_freq
}

/// Converts ticks to nanoseconds.
///
/// # Arguments:
///
/// * `ticks`: Ticks to convert.
#[inline]
pub const fn ticks_to_nanoseconds(ticks: u64) -> u64 {
    (ticks * 625) / 12
}

/// Converts nanoseconds to ticks.
///
/// # Arguments:
///
/// * `ns`: Nanoseconds to convert.
#[inline]
pub const fn nanoseconds_to_ticks(ns: u64) -> u64 {
    (ns * 12) / 625
}
