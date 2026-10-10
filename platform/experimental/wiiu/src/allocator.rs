use core::alloc::{GlobalAlloc, Layout};

unsafe extern "C" {
    fn malloc(size: usize) -> *mut u8;
    fn free(ptr: *mut u8);
}

struct CAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: CAllocator = CAllocator;

unsafe impl GlobalAlloc for CAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(layout.align());
        unsafe { malloc(size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if !ptr.is_null() {
            unsafe {
                free(ptr);
            }
        }
    }
}

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    unsafe {
        crate::host::rfvp_wiiu_platform_fatal(
            0xFFFF_0001,
            b"rfvp-wiiu allocation failed".as_ptr(),
            b"rfvp-wiiu allocation failed".len(),
        );
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    unsafe {
        crate::host::rfvp_wiiu_platform_fatal(
            0xFFFF_0002,
            b"rfvp-wiiu panic".as_ptr(),
            b"rfvp-wiiu panic".len(),
        );
    }
}
