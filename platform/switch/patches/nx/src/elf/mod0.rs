//! MOD0 format utils

/// Represents the `MOD0` start layout, which are the first 8 bytes of the binary in memory.
/// These are usually right before the actual header in official binaries, but they don't have
/// to be and we store it (the actual header) in `.rodata`.
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ModuleStart {
    /// Reserved
    pub reserved: u32,
    /// The magic offset
    pub magic_offset: u32,
}

/// Represents the `MOD0` header structure.
/// Although we know from the official linker script that all the offsets will be positive,
/// the offsets have been made signed so that consumers can bring their own linker scripts
/// (e.g. emuiibo) and we won't break functionality if they reorder sections.
///
/// All members have been made private as this should only ever be instantiated using
/// [`Header::from_text_start_addr`], with data populated by the linker.
#[derive(Copy, Clone)]
#[repr(C)]
pub struct Header {
    /// The magic, whose expected value is [`MAGIC`][`Header::MAGIC`]
    magic: u32,
    /// The dynamic offset
    dynamic: i32,
    /// The BSS start offset
    bss_start: i32,
    /// The BSS end offset
    bss_end: i32,
    /// The eh_frame_hdr start offset
    eh_frame_hdr_start: i32,
    /// The eh_frame_hdr end offset
    eh_frame_hdr_end: i32,
    /// The offset to runtime-generated module object
    module_object: i32,
}

impl Header {
    /// The header magic value (`MOD0`)
    pub const MAGIC: u32 = u32::from_le_bytes(*b"MOD0");

    /// Gets the header embedded at the slot `.text.jmp+4`.
    /// Since this is a hard requirement of the switch runtime,
    /// an invalid MOD0 header offset or invalid header magic value panics.
    /// Panics if the `text_base` pointer is invalid, the derived `MOD0`
    /// pointer is invalid, or if the derived `MOD0` header magic value is incorrect.
    ///
    /// # Arguments:
    ///
    /// * `text_base`: The start of the `.text.jmp` section.
    #[inline]
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // We are checking the validity of the pointer, so this is handled by
    pub fn from_text_start_addr(text_base: *mut u8) -> &'static mut Self {
        let mod0_ref = unsafe {
            let module_start = (text_base as *const ModuleStart)
                .as_ref()
                .expect("Invalid base `.text` pointer. Address is null or improperly aligned");

            // Get the MOD0 offset that is written at the slot `.text.jmp+4`
            let mod0_offset = module_start.magic_offset as usize;

            // The mod0_ptr is written into `.rodata`, at the offset `mod0_offset` from the start of `.test.jmp`
            let mod0_ptr = text_base.add(mod0_offset) as *mut Header;

            mod0_ptr.as_mut().expect(
                "Failed to get reference to MOD0 header. Address is null or improperly aligned.",
            )
        };

        assert!(mod0_ref.magic == Header::MAGIC, "Invalid MOD0 magic value.");

        mod0_ref
    }

    /// Gets the start address for the `.dynamic` section
    pub fn get_dyn_start(&self) -> *const super::Dyn {
        // This could cause panics on access if the pointer is incorrectly aligned but that is not a
        // UB issue here - unaligned raw pointers are allowed.
        unsafe {
            (self as *const Self as *const u8).offset(self.dynamic as isize) as *const super::Dyn
        }
    }

    /// Gets the start address for the `eh_frame_hdr` section.
    ///
    /// # Safety
    ///
    /// The reference `&self` must be the copy in `.rodata` created by the linker.
    #[inline]
    pub fn get_eh_frame_header_start(&self) -> *const u8 {
        // SAFETY: Safe as we are just computing a new pointer, not dereferencing.
        unsafe { (self as *const Self as *const u8).offset(self.eh_frame_hdr_start as isize) }
    }

    /// Zeroes the bss section from a base code address. We have to take an `&mut self` here as computing.
    ///
    /// # Safety
    ///
    /// The reference `&mut self` must be the copy in `.rodata` created by the linker. Additionally,
    /// The reference to self should have been created using a mutable pointer, to prevent a shared->mut conversion
    /// which would be immediate UB (as documented in the struct docstring).
    #[inline]
    pub unsafe fn zero_bss_section(&mut self) {
        use zeroize::Zeroize;

        debug_assert!(
            self.bss_end >= self.bss_start,
            "Invalid offset range for BSS region. End address is before start address."
        );

        unsafe {
            let module_addr = (&raw mut *self).cast::<u8>();
            let bss_start = module_addr.offset(self.bss_start as isize);
            let bss_len = (self.bss_end - self.bss_start) as usize;

            // Use the zeroize library to get bss zeroing with the guarantee that it won'get get elided.
            core::slice::from_raw_parts_mut(bss_start, bss_len).zeroize();
        }

        // With a SeqCst fence, we ensure that the bss section is zeroed before we return.
        // The call to this function now can not be reordered either.
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    }
}
