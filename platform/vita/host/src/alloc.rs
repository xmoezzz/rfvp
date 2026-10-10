use core::alloc::{GlobalAlloc, Layout};

extern "C" {
    fn rfvp_psv_alloc(size: usize, align: usize) -> *mut u8;
    fn rfvp_psv_dealloc(ptr: *mut u8, size: usize, align: usize);
}

pub struct PsvGlobalAllocator;

unsafe impl GlobalAlloc for PsvGlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { rfvp_psv_alloc(layout.size(), layout.align()) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { rfvp_psv_dealloc(ptr, layout.size(), layout.align()) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: PsvGlobalAllocator = PsvGlobalAllocator;
