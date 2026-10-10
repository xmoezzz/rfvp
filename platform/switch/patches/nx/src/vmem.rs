//! Virtual memory support

use core::ptr::null_mut;
use core::sync::atomic::AtomicPtr;

use crate::sync::RwLock;

use crate::result::*;
use crate::svc;

/// Represents a virtual region of memory, represented as pointer-sized uints. i.e. [start, end)
#[derive(Copy, Clone, Default, Debug)]
pub struct VirtualRegion {
    /// The start address of the region
    pub start: usize,
    /// The (non inclusive) end address of the region
    pub end: usize,
}

impl VirtualRegion {
    /// Creates an empty [`VirtualRegion`] with invalid address values
    #[inline]
    pub const fn null() -> Self {
        Self { start: 0, end: 0 }
    }

    /// Returns whether the specified address is contained in the region
    ///
    /// # Arguments
    ///
    /// * `address` - The address to check
    #[inline]
    pub const fn contains_addr(&self, address: usize) -> bool {
        (address >= self.start) && (address < self.end)
    }

    /// Returns whether the specified region is fully contained in the region
    ///
    /// # Arguments
    ///
    /// * `other` - The region to check
    #[inline]
    pub const fn contains(&self, other: Self) -> bool {
        self.start <= other.start && self.end >= other.end
    }

    /// Returns whether the other region overlaps this instance
    ///
    /// # Arguments
    ///
    /// * `other` - The other region to check
    #[inline]
    pub const fn overlaps(&self, other: Self) -> bool {
        !(other.end <= self.start || self.end <= other.start)
    }
}

/// The standard memory regions for NX processes
#[derive(Copy, Clone, Debug, Default)]
pub struct StandardRegions {
    stack: VirtualRegion,
    heap: VirtualRegion,
    legacy_alias: VirtualRegion,
    global_address_space: VirtualRegion,
}

impl StandardRegions {
    const fn new() -> Self {
        Self {
            stack: VirtualRegion::null(),
            heap: VirtualRegion::null(),
            legacy_alias: VirtualRegion::null(),
            global_address_space: VirtualRegion::null(),
        }
    }
}

static STANDARD_VMEM_REGIONS: RwLock<StandardRegions> = RwLock::new(StandardRegions::new());
static NEXT_FREE_PTR: AtomicPtr<u8> = AtomicPtr::new(null_mut());

/// Gets the current process's address space [`VirtualRegion`]
///
/// Note that [`initialize()`] must have been called before for the region to be valid (although it's automatically called on [`rrt0`][`crate::rrt0`])
pub fn get_address_space() -> VirtualRegion {
    STANDARD_VMEM_REGIONS.read().global_address_space
}

/// Gets the current process's stack [`VirtualRegion`]
///
/// Note that [`initialize()`] must have been called before for the region to be valid (although it's automatically called on [`rrt0`][`crate::rrt0`])
pub fn get_stack_region() -> VirtualRegion {
    STANDARD_VMEM_REGIONS.read().stack
}

/// Gets the current process's heap [`VirtualRegion`]
///
/// Note that [`initialize()`] must have been called before for the region to be valid (although it's automatically called on [`rrt0`][`crate::rrt0`])
pub fn get_heap_region() -> VirtualRegion {
    STANDARD_VMEM_REGIONS.read().heap
}

/// Gets the current process's legacy alias [`VirtualRegion`]
///
/// Note that [`initialize()`] must have been called before for the region to be valid (although it's automatically called on [`rrt0`][`crate::rrt0`])
pub fn get_legacy_alias_region() -> VirtualRegion {
    STANDARD_VMEM_REGIONS.read().legacy_alias
}

fn read_region_info(
    address_info_id: svc::InfoId,
    size_info_id: svc::InfoId,
) -> Result<VirtualRegion> {
    let start = svc::get_info(address_info_id, svc::CURRENT_PROCESS_PSEUDO_HANDLE, 0)? as usize;
    let size = svc::get_info(size_info_id, svc::CURRENT_PROCESS_PSEUDO_HANDLE, 0)? as usize;

    Ok(VirtualRegion {
        start,
        end: start + size,
    })
}

/// Initializes virtual memory support
///
/// This internally retrieves all the current process's memory [`VirtualRegion`]s
///
/// This is automatically called on [`rrt0`][`crate::rrt0`]
pub fn initialize() -> Result<()> {
    use svc::InfoId::*;

    *STANDARD_VMEM_REGIONS.write() = StandardRegions {
        global_address_space: read_region_info(AslrRegionAddress, AslrRegionSize)?,
        stack: read_region_info(StackRegionAddress, StackRegionSize)?,
        heap: read_region_info(HeapRegionAddress, HeapRegionSize)?,
        legacy_alias: read_region_info(AliasRegionAddress, AliasRegionSize)?,
    };

    Ok(())
}

/// Finds available virtual memory for the specified size, returning it's address
///
/// Note that [`initialize()`] must have been called before for this to succeed (although it's automatically called on [`rrt0`][`crate::rrt0`])
///
/// # Arguments
///
/// * `size`: The size of the virtual memory to allocate
pub fn allocate(size: usize) -> Result<*mut u8> {
    use core::sync::atomic::Ordering::*;

    let vmem_regions = *STANDARD_VMEM_REGIONS.read();
    let original_free_ptr = NEXT_FREE_PTR.load(Relaxed);
    let mut attempt_addr = original_free_ptr as usize;

    loop {
        if !vmem_regions
            .global_address_space
            .contains_addr(attempt_addr)
        {
            attempt_addr = vmem_regions.global_address_space.start;
        }

        let attempt_region = VirtualRegion {
            start: attempt_addr,
            end: attempt_addr + size,
        };
        if vmem_regions.stack.overlaps(attempt_region) {
            attempt_addr = vmem_regions.stack.end;
            continue;
        }

        if vmem_regions.heap.overlaps(attempt_region) {
            attempt_addr = vmem_regions.heap.end;
            continue;
        }

        if vmem_regions.legacy_alias.overlaps(attempt_region) {
            attempt_addr = vmem_regions.legacy_alias.end;
            continue;
        }

        // we have an address that isn't in a predefined region. So now we're going to just check if it's already mapped for something
        match svc::query_memory(attempt_addr as *mut u8)? {
            (memory_info, _) if memory_info.state == svc::MemoryState::Free => {
                match NEXT_FREE_PTR.compare_exchange(
                    original_free_ptr,
                    attempt_addr as *mut u8,
                    SeqCst,
                    SeqCst,
                ) {
                    Ok(_) => {
                        return Ok(attempt_addr as *mut u8);
                    }
                    Err(new_attempt_addr) => {
                        attempt_addr = new_attempt_addr as usize;
                        continue;
                    }
                }
            }
            (memory_info, _) => {
                attempt_addr = memory_info.base_address + memory_info.size;
                continue;
            }
        }
    }
}
