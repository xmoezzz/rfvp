use core::arch::naked_asm as nasm;

use crate::result::ResultCode;
use crate::svc::{CreateProcessInfo, DebugThreadParam, SystemInfoParam};
use crate::{arm, svc::PhysicalMemoryInfo};

use super::{
    BreakReason, CodeMapOperation, DebugEvent, Handle, InfoId, LastThreadContext,
    LimitableResource, MemoryAttribute, MemoryInfo, MemoryPermission, PageInfo, SchedulerState,
};

#[unsafe(naked)]
pub unsafe extern "C" fn set_heap_size(out_address: *mut *mut u8, size: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x1",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_memory_permission(
    address: *const u8,
    size: usize,
    value: MemoryPermission,
) -> ResultCode {
    nasm!("svc 0x2", "ret")
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_memory_attribute(
    address: *const u8,
    size: usize,
    mask: u32,
    value: MemoryAttribute,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x3",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_memory(
    address: *const u8,
    source_address: *mut u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x4",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_memory(
    address: *const u8,
    source_address: *mut u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x5",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn query_memory(
    out_info: *mut MemoryInfo,
    out_page_info: *mut PageInfo,
    address: *const u8,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x1, [sp, #-16]!",
        "svc 0x6",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn exit_process() -> ! {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x7",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_thread(
    handle: *mut Handle,
    entry: unsafe extern "C" fn(*mut u8) -> !,
    entry_arg: *const u8,
    stack_top: *const u8,
    priority: i32,
    processor_id: i32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x8",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn start_thread(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x9",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn exit_thread() -> ! {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0xA",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn sleep_thread(timeout: i64) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0xB",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_thread_priority(out_priority: *mut i32, handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0xC",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_thread_priority(handle: Handle, priority: i32) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0xD",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_thread_core_mask(
    core_mask: *mut i32,
    core_affinity: *mut u64,
    handle: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "stp x0, x1, [sp, #-16]!",
        "svc 0xE",
        "ldp x3, x4, [sp], #16",
        "str w1, [x3]",
        "str x2, [x4]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_thread_core_mask(
    handle: Handle,
    preferred_core: i32,
    affinity_mask: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0xF",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_current_processor_number() -> u32 {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x10",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn signal_event(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x11",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn clear_event(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x12",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_shared_memory(
    handle: Handle,
    address: *const u8,
    size: usize,
    permission: MemoryPermission,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x13",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_shared_memory(
    handle: Handle,
    address: *const u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x14",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_transfer_memory(
    out_handle: *mut Handle,
    address: *const u8,
    size: usize,
    permissions: MemoryPermission,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x15",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn close_handle(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x16",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn reset_signal(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x17",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn wait_synchronization(
    out_index: *mut i32,
    handles: *const Handle,
    handle_count: u32,
    timeout: i64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x18",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn cancel_synchronization(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x19",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn arbitrate_lock(
    thread_handle: Handle,
    tag_location: *const u8,
    tag: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x1A",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn arbitrate_unlock(tag_location: *const u8) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x1B",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn wait_process_wide_key_atomic(
    wait_location: *const u8,
    tag_location: *const u8,
    desired_tag: u32,
    timeout: i64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x1C",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn signal_process_wide_key(
    tag_location: *const u8,
    desired_tag: i32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x1D",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_system_tick() -> u64 {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x1E",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn connect_to_named_port(
    out_handle: *mut Handle,
    name: *const u8,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x1F",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn send_sync_request_light(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x20",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn send_sync_request(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x21",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn send_sync_request_with_user_data(
    buffer: *mut u8,
    size: usize,
    session: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x22",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn send_async_request_with_user_data(
    handle: *mut Handle,
    buffer: *mut u8,
    size: usize,
    session: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x23",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_process_id(
    out_process_id: *mut u64,
    process_handle: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x24",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_thread_id(out_thread_id: *mut u64, handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x25",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn r#break(reason: BreakReason, arg: *const u8, size: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x26",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn output_debug_string(msg: *const u8, len: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x27",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn return_from_exception(res: ResultCode) -> ! {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x28",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_info(
    out_info: *mut u64,
    id: InfoId,
    handle: Handle,
    sub_id: u64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x29",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn flush_entire_data_cache() -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x2A",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn flush_data_cache(address: *const u8, len: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x2B",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_physical_memory(address: *const u8, len: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x2C",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_physical_memory(address: *const u8, len: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x2D",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_debug_future_thread_info(
    out_context: *mut LastThreadContext,
    out_thread_id: *mut u64,
    debug_proc_handle: Handle,
    ns: i64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "stp x0, x1, [sp, #-16]!",
        "svc 0x2E",
        "ldp x6, x7, [sp], #16",
        "stp x1, x2, [x6]",
        "stp x3, x4, [x6, #16]",
        "str x5, [x7]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_last_thread_info(
    out_context: *mut LastThreadContext,
    out_tls_address: *mut u64,
    out_flags: *mut u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "stp x1, x2, [sp, #-16]!",
        "str x0, [sp, #-16]!",
        "svc 0x2F",
        "ldr x7, [sp], #16",
        "stp x1, x2, [x7]",
        "stp x3, x4, [x7, #16]",
        "ldp x1, x2, [sp], #16",
        "str x5, [x1]",
        "str w6, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_resource_limit_limit_value(
    out_val: *mut i64,
    resource_limit_handle: Handle,
    limit_kind: LimitableResource,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x30",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_resource_limit_current_value(
    out_val: *mut i64,
    resource_limit_handle: Handle,
    limit_kind: LimitableResource,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x31",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_thread_activity(
    thread_handle: Handle,
    thread_state: SchedulerState,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x32",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_thread_context3(
    out_context: *mut arm::ThreadContext,
    thread_handle: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x32",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn wait_for_address(
    address: *const u8,
    arbitration_type: u32,
    value: u32,
    timeout: i64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x34",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn signal_to_address(
    address: *const u8,
    signal: u32,
    value: u32,
    signal_count: i32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x35",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn synchronize_preemption_states() -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x36",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_resource_limit_peak_value(
    out_value: *mut i64,
    resource_limit_handle: Handle,
    limit_kind: LimitableResource,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x37",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_io_pool(pool_type: u32) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x39",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_io_region(
    io_region_handle: *mut Handle,
    io_pool_handle: Handle,
    physical_addres: *mut u8,
    size: usize,
    permissions: MemoryPermission,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x3A",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn kernel_debug(
    debug_type: u32,
    debug_arg0: u64,
    debug_arg1: u64,
    debug_arg2: u64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x3C",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn change_kernel_trace_state(tracing_state: u32) {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x3D",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_session(
    out_server_handle: *mut Handle,
    out_client_handle: *mut Handle,
    is_light: bool,
    unk_name: u64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "stp x0, x1, [sp, #-16]!",
        "svc 0x40",
        "ldp x3, x4, [sp], #16",
        "str w1, [x3]",
        "str w2, [x4]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn accept_session(
    out_session_handle: *mut Handle,
    handle: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x41",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn reply_and_receive_light(handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x42",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn reply_and_receive(
    out_index: *mut i32,
    handles: *const Handle,
    handle_count: u32,
    reply_target: Handle,
    timeout: i64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x43",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn reply_and_receive_with_user_buffer(
    out_index: *mut i32,
    user_buffer: *mut u8,
    buffer_size: usize,
    handles: *const Handle,
    handle_count: u32,
    reply_target: Handle,
    timeout: i64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x44",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_event(
    out_server_handle: *mut Handle,
    out_client_handle: *mut Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "stp x0, x1, [sp, #-16]!",
        "svc 0x45",
        "ldp x3, x4, [sp], #16",
        "str w1, [x3]",
        "str w2, [x4]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_io_region(
    io_region_handle: Handle,
    address: *mut u8,
    size: usize,
    permissions: MemoryPermission,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x46",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_io_region(
    io_region_handle: Handle,
    address: *mut u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x47",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_physical_memory_unsafe(address: *mut u8, size: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x48",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_physical_memory_unsafe(address: *mut u8, size: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x49",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_unsafe_limit(size: usize) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x4A",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_code_memory(
    code_memory_handle: *mut Handle,
    source_address: *mut u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x4B",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn control_code_memory(
    code_memory_handle: Handle,
    operation_type: CodeMapOperation,
    destination_address: *mut u8,
    size: usize,
    permission: MemoryPermission,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x4C",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn sleep_system() -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x4D",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn read_write_register(
    mmio_val: u32,
    register_addres: usize,
    read_write_mask: u32,
    in_val: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x4E",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_process_activity(
    process: Handle,
    paused: SchedulerState,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x4F",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_shared_memory(
    shmem_handle: *mut Handle,
    size: usize,
    local_permission: MemoryPermission,
    other_permission: MemoryPermission,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x50",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_transfer_memory(
    tmem_handle: Handle,
    address: *mut u8,
    size: usize,
    permissions: MemoryPermission,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x51",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_transfer_memory(
    tmem_handle: Handle,
    address: *mut u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x52",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_interrupt_event(
    int_handle: *mut Handle,
    irq_number: u64,
    flags: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x53",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn query_physical_address(
    mem_info: PhysicalMemoryInfo,
    virtual_address: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x54",
        "ldr x4, [sp], #16",
        "stp x1, x2, [x4]",
        "str x3, [x4, #16]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn query_io_mapping(
    virtual_address: *mut usize,
    virtual_size: *mut usize,
    physical_address: usize,
    physical_size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "stp x0, x1, [sp, #-16]!",
        "svc 0x55",
        "ldp x3, x4, [sp], #16",
        "str x1, [x3]",
        "str x2, [x4]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn legacy_query_io_mapping(
    virtual_address: *mut usize,
    physical_address: usize,
    physical_size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x55",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_device_address_space(
    device_handle: *mut Handle,
    device_address: usize,
    device_mem_size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x56",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn attach_device_address_space(
    device: usize,
    device_handle: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x57",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn detach_device_address_space(
    device: usize,
    device_handle: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x58",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_device_address_space_by_force(
    handle: Handle,
    process_handle: Handle,
    map_addresss: usize,
    device_mem_size: usize,
    device_address: usize,
    options: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x59",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_device_address_space_aligned(
    handle: Handle,
    process_handle: Handle,
    map_addresss: usize,
    device_mem_size: usize,
    device_address: usize,
    options: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x5A",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_device_address_space(
    mapped_address_size: *mut usize,
    handle: Handle,
    process_handle: Handle,
    map_addresss: usize,
    device_mem_size: usize,
    device_address: usize,
    options: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x5B",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_device_address_space(
    handle: Handle,
    process_handle: Handle,
    map_addresss: usize,
    device_mem_size: usize,
    device_address: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x5C",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn invalidate_process_data_cache(
    proc_handle: Handle,
    address: *const u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x5D",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn store_process_data_cache(
    proc_handle: Handle,
    address: *const u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x5E",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn flush_process_data_cache(
    proc_handle: Handle,
    address: *const u8,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x5F",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn debug_active_process(
    out_handle: *mut Handle,
    process_id: u64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x60",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn break_debug_process(debug_handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x61",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn terminate_debug_process(debug_handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x62",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_debug_event(
    out_debug_event: *mut DebugEvent,
    debug_handle: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x63",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn continue_debug_event(
    debug_handle: Handle,
    flags: u32,
    thread_ids: *const u64,
    thread_id_count: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x64",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_process_list(
    out_count: *mut u32,
    out_process_ids: *mut u64,
    process_id_count: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x65",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_thread_list(
    out_count: *mut u32,
    out_thread_ids: *mut u64,
    thread_id_count: u32,
    debug_handle: Handle,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x66",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_debug_thread_context(
    thread_context: *mut arm::ThreadContext, // *mut arm::ThreadContext
    debug_handle: Handle,
    thread_id: u64,
    register_group: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x67",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_debug_thread_context(
    debug_handle: Handle,
    thread_id: u64,
    thread_context: *const arm::ThreadContext,
    register_group: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x68",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn query_debug_process_memory(
    out_info: *mut MemoryInfo,
    out_page_info: *mut PageInfo,
    debug_handle: Handle,
    address: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x1, [sp, #-16]!",
        "svc 0x69",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn read_debug_process_memory(
    buffer: *mut u8,
    debug_handle: Handle,
    address: usize,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x6A",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn write_debug_process_memory(
    debug_handle: Handle,
    buffer: *const u8,
    address: usize,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x6B",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_hardware_break_point(
    which: u32,
    flags: u64,
    value: u64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x6C",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_debug_thread_param(
    out_64: *mut u64,
    out_32: *mut u32,
    debug_handle: Handle,
    thread_id: u64,
    param: DebugThreadParam,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "stp x0, x1, [sp, #-16]!",
        "svc 0x6D",
        "ldp x3, x4, [sp], #16",
        "str x1, [x3]",
        "str w2, [x4]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_system_info(
    out_info: *mut u64,
    id0: SystemInfoParam,
    handle: Handle,
    id1: u64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x6F",
        "ldr x2, [sp], #16",
        "str x1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_named_port(
    server_handle: *mut Handle,
    client_handle: *mut Handle,
    max_sessions: i32,
    is_light: bool,
    name: *const u8,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "stp x0, x1, [sp, #-16]!",
        "svc 0x70",
        "ldp x3, x4, [sp], #16",
        "str w1, [x3]",
        "str w2, [x4]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn manage_named_port(
    out_handle: *mut Handle,
    name: *const u8,
    max_sessions: i32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x71",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn connect_to_port(session: *mut Handle, port_handle: Handle) -> ResultCode {
    {
        nasm!(
            maybe_cfi!(".cfi_startproc"),
            "str x0, [sp, #-16]!",
            "svc 0x72",
            "ldr x2, [sp], #16",
            "str w1, [x2]",
            "ret",
            maybe_cfi!(".cfi_endproc")
        );
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_process_memory_permission(
    process_handle: Handle,
    address: usize,
    size: usize,
    permissions: MemoryPermission,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x73",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_process_memory(
    destination_address: *mut u8,
    proc_handle: Handle,
    source_address: usize,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x74",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_process_memory(
    destination_address: *mut u8,
    proc_handle: Handle,
    source_address: usize,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x75",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn query_process_memory(
    out_memory_info: *mut MemoryInfo,
    out_page_info: *mut PageInfo,
    proc_handle: Handle,
    address: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x1, [sp, #-16]!",
        "svc 0x76",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn map_process_code_memory(
    proc_handle: Handle,
    destination_address: usize,
    source_address: usize,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x77",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn unmap_process_code_memory(
    proc_handle: Handle,
    destination_address: usize,
    source_address: usize,
    size: usize,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x78",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_process(
    out_handle: *mut Handle,
    proc_info: *const CreateProcessInfo,
    capabilities: *const u32,
    capability_count: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x79",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn start_process(
    proc_handle: Handle,
    main_thread_priority: i32,
    default_cpu_core: i32,
    main_thread_stack_size: u32,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x7A",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn terminate_process(proc_handle: Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x7B",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn get_process_info(out_info: *mut i64) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x7C",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn create_resource_limit(out_handle: *mut Handle) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "svc 0x7D",
        "ldr x2, [sp], #16",
        "str w1, [x2]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn set_resource_limit_limit_value(
    resource_limit_handle: Handle,
    limit_kind: LimitableResource,
    value: u64,
) -> ResultCode {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "svc 0x7E",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}

#[unsafe(naked)]
pub unsafe extern "C" fn call_secure_monitor(args: *mut u64) {
    nasm!(
        maybe_cfi!(".cfi_startproc"),
        "str x0, [sp, #-16]!",
        "mov x8, x0",
        "ldp x0, x1, [x8]",
        "ldp x2, x3, [x8, #0x10]",
        "ldp x4, x5, [x8, #0x20]",
        "ldp x6, x7, [x8, #0x30]",
        "svc 0x7F",
        "ldr x8, [sp], #16",
        "stp x0, x1, [x8]",
        "stp x2, x3, [x8, #0x10]",
        "stp x4, x5, [x8, #0x20]",
        "stp x6, x7, [x8, #0x30]",
        "ret",
        maybe_cfi!(".cfi_endproc")
    );
}
