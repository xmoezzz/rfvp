//! Memory (heap) support and utils

use crate::result::ResultBase;
use crate::svc;
pub mod alloc;

/// Blocks thread until the memory region specified has the permission passed
///
/// # Arguments
///
/// * `address`: The address to query for memory permissions
/// * `permissions`: The memory permission to wait on
///
/// Note that if multiple permissions are specified (e.g. `MemoryPermission::Read | MemoryPermission::Write`), the function will return if *any* specified permission is present.
#[inline(always)]
pub fn wait_for_permission(
    address: svc::Address,
    permission: svc::MemoryPermission,
    timeout: Option<usize>,
) -> crate::result::Result<()> {
    let timeout = timeout.unwrap_or(usize::MAX);
    let mut time_taken: usize = 0;

    while !svc::query_memory(address)?
        .0
        .permission
        .intersects(permission)
    {
        result_return_if!(time_taken >= timeout, svc::rc::ResultTimedOut);
        time_taken = time_taken.saturating_add(100_000);
        let _ = crate::thread::sleep(100_000);
    }

    Ok(())
}
