//! The heap: one block reserved from the kernel the first time memory is needed, managed by talc.
//!
//! The psp crate's own allocator asks the kernel for a block per allocation, which is slow, costs
//! a kernel object each and rounds every allocation up to 256 bytes (see
//! platform/psp/patches/README.md).

use core::alloc::Layout;
use core::ptr;

use talc::{OomHandler, Span, Talc, Talck};

/// Left to the kernel for thread stacks (the audio thread's included) and the system.
const KERNEL_RESERVE: usize = 1024 * 1024;

/// Claims the heap on the first allocation that does not fit (the first one of all).
pub struct ClaimOnce {
    claimed: bool,
}

impl OomHandler for ClaimOnce {
    fn handle_oom(talc: &mut Talc<Self>, layout: Layout) -> Result<(), ()> {
        if talc.oom_handler.claimed {
            let counters = talc.get_counters();
            write_oom_report(layout.size(), counters.allocated_bytes, counters.claimed_bytes);
            return Err(());
        }
        talc.oom_handler.claimed = true;
        unsafe {
            let free = psp::sys::sceKernelMaxFreeMemSize() as usize;
            let size = free.saturating_sub(KERNEL_RESERVE) & !0xFFF;
            if size == 0 {
                return Err(());
            }
            let id = psp::sys::sceKernelAllocPartitionMemory(
                psp::sys::SceSysMemPartitionId::SceKernelPrimaryUserPartition,
                b"rfvp_heap\0".as_ptr(),
                psp::sys::SceSysMemBlockTypes::Low,
                size as u32,
                ptr::null_mut(),
            );
            if id.0 < 0 {
                return Err(());
            }
            let base: *mut u8 = psp::sys::sceKernelGetBlockHeadAddr(id).cast();
            talc.claim(Span::from_base_size(base, size)).map(|_| ())
        }
    }
}

#[global_allocator]
static ALLOCATOR: Talck<crate::lock::YieldingMutex, ClaimOnce> =
    Talc::new(ClaimOnce { claimed: false }).lock();

/// Bytes allocated now, the most allocated at once so far, and the size of the heap.
pub fn usage() -> (usize, usize, usize) {
    let talc = ALLOCATOR.lock();
    let counters = talc.get_counters();
    (
        counters.allocated_bytes,
        PEAK.load(core::sync::atomic::Ordering::Relaxed).max(counters.allocated_bytes),
        counters.claimed_bytes,
    )
}

/// Note the current allocation as a candidate for the peak (called once a frame).
pub fn note_peak() {
    let allocated = ALLOCATOR.lock().get_counters().allocated_bytes;
    PEAK.fetch_max(allocated, core::sync::atomic::Ordering::Relaxed);
}

static PEAK: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Write `last_oom.txt`: what could not be allocated, and how full the heap was. It allocates
/// nothing, since there is no memory left.
fn write_oom_report(size: usize, allocated: usize, heap: usize) {
    let mut text = [0u8; 96];
    let mut len = 0;
    for (label, value) in [("wanted ", size), (" bytes; heap used ", allocated), (" of ", heap)] {
        for byte in label.bytes() {
            text[len] = byte;
            len += 1;
        }
        let mut digits = [0u8; 20];
        let mut n = value;
        let mut count = 0;
        loop {
            digits[count] = b'0' + (n % 10) as u8;
            count += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        while count > 0 {
            count -= 1;
            text[len] = digits[count];
            len += 1;
        }
    }
    unsafe {
        let fd = psp::sys::sceIoOpen(
            b"last_oom.txt\0".as_ptr(),
            psp::sys::IoOpenFlags::WR_ONLY | psp::sys::IoOpenFlags::CREAT | psp::sys::IoOpenFlags::TRUNC,
            0o777,
        );
        if fd.0 >= 0 {
            psp::sys::sceIoWrite(fd, text.as_ptr().cast(), len);
            psp::sys::sceIoClose(fd);
        }
    }
}
