//! This module wraps svc calls provided by `asm.rs`.
//! There is generally no function-level Safety docs, but the core requirement is that all raw pointers provided must be
//! validated by the caller.

use crate::arm;
use crate::ipc::sf::ncm;
use crate::result::*;
use crate::util;
use crate::util::ArrayString;
use core::mem;
use core::ptr;

pub mod asm;
pub mod rc;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ArbitrationType {
    WaitIfLessThan = 0,
    DecrementAndWaitIfLessThan = 1,
    WaitIfEqual = 2,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum SignalType {
    Signal = 0,
    SignalAndIncrementIfEqual = 1,
    SignalAndModifyBasedOnWaitingThreadCountIfEqual = 2,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum BreakReason {
    Panic = 0,
    Assert = 1,
    User = 2,
    PreLoadDll = 3,
    PostLoadDll = 4,
    PreUnloadDll = 5,
    PostUnloadDll = 6,
    CppException = 7,
    NotificationOnlyFlag = 0x80000000,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum CodeMapOperation {
    MapOwner = 0,
    MapSlave,
    UnmapOwner,
    UnmapSlave,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
pub enum MemoryState {
    #[default]
    Free = 0x0,
    Io = 0x1,
    Static = 0x2,
    Code = 0x3,
    CodeData = 0x4,
    Normal = 0x5,
    Shared = 0x6,
    Alias = 0x7,
    AliasCode = 0x8,
    AliasCodeData = 0x9,
    Ipc = 0xA,
    Stack = 0xB,
    ThreadLocal = 0xC,
    Transferred = 0xD,
    SharedTransferred = 0xE,
    SharedCode = 0xF,
    Inaccessible = 0x10,
    NonSecureIpc = 0x11,
    NonDeviceIpc = 0x12,
    Kernel = 0x13,
    GeneratedCode = 0x14,
    CodeOut = 0x15,
}

define_bit_set! {
    MemoryPermission (u32) {
        None = 0,
        Read = bit!(0),
        Write = bit!(1),
        Execute = bit!(2),
        DontCare = bit!(28)
    }
}

define_bit_set! {
    MemoryAttribute (u32) {
        None = 0,
        Borrowed = bit!(0),
        IpcMapped = bit!(1),
        DeviceMapped = bit!(2),
        Uncached = bit!(3)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct MemoryInfo {
    pub base_address: usize,
    pub size: usize,
    pub state: MemoryState,
    pub attribute: MemoryAttribute,
    pub permission: MemoryPermission,
    pub ipc_refcount: u32,
    pub device_refcount: u32,
    pub pad: u32,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum InfoId {
    /// Bitmask of allowed Core IDs.
    CoreMask = 0,
    /// Bitmask of allowed Thread Priorities.
    PriorityMask = 1,
    /// Base of the Alias memory region.
    AliasRegionAddress = 2,
    /// Size of the Alias memory region.
    AliasRegionSize = 3,
    /// Base of the Heap memory region.
    HeapRegionAddress = 4,
    /// Size of the Heap memory region.
    HeapRegionSize = 5,
    /// Total amount of memory available for process.
    TotalMemorySize = 6,
    /// Amount of memory currently used by process.
    UsedMemorySize = 7,
    /// Whether current process is being debugged.
    DebuggerAttached = 8,
    /// Current process's resource limit handle.
    ResourceLimit = 9,
    /// Number of idle ticks on CPU.
    IdleTickCount = 10,
    /// [2.0.0+] Random entropy for current process.
    RandomEntropy = 11,
    /// [2.0.0+] Base of the process's address space.
    AslrRegionAddress = 12,
    /// [2.0.0+] Size of the process's address space.
    AslrRegionSize = 13,
    /// [2.0.0+] Base of the Stack memory region.
    StackRegionAddress = 14,
    /// [2.0.0+] Size of the Stack memory region.
    StackRegionSize = 15,
    /// [3.0.0+] Total memory allocated for process memory management.
    SystemResourceSizeTotal = 16,
    /// [3.0.0+] Amount of memory currently used by process memory management.
    SystemResourceSizeUsed = 17,
    /// [3.0.0+] Program ID for the process.
    ProgramId = 18,
    /// [4.0.0-4.1.0] Min/max initial process IDs.
    InitialProcessIdRange = 19,
    /// [5.0.0+] Address of the process's exception context (for break).
    UserExceptionContextAddress = 20,
    /// [6.0.0+] Total amount of memory available for process, excluding that for process memory management.
    TotalNonSystemMemorySize = 21,
    /// [6.0.0+] Amount of memory used by process, excluding that for process memory management.
    UsedNonSystemMemorySize = 22,
    /// [9.0.0+] Whether the specified process is an Application.
    IsApplication = 23,
    /// [11.0.0+] The number of free threads available to the process's resource limit.
    FreeThreadCount = 24,
    /// [13.0.0+] Number of ticks spent on thread.
    ThreadTickCount = 25,
    /// [14.0.0+] Does process have access to SVC (only usable with \ref svcSynchronizePreemptionState at present).
    IsSvcPermitted = 26,
    /// [16.0.0+] Low bits of the physical address for a KIoRegion.
    IoRegionHint = 27,
    /// [18.0.0+] Extra size added to the reserved region.
    AliasRegionExtraSize = 28,
    /// [19.0.0+] Low bits of the process address for a KTransferMemory.
    TransferMemoryHint = 34,
    /// [1.0.0-12.1.0] Number of ticks spent on thread.
    ThreadTickCountDeprecated = 0xF0000002,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum SystemInfoParam {
    /// Total amount of DRAM available to system.
    TotalPhysicalMemorySize = 0,
    /// Current amount of DRAM used by system.
    UsedPhysicalMemorySize = 1,
    /// Min/max initial process IDs.
    InitialProcessIdRange = 2,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum AddressSpaceType {
    ThirtyTwoBit = 0,
    SixtyFourBitDeprecated = 1,
    ThirtyTwoBitWithoutAlias = 2,
    SixtyFourBit = 3,
    #[default]
    Mask = 0x7,
}

impl AddressSpaceType {
    const fn into_bits(self) -> u8 {
        self as _
    }

    const fn from_bits(val: u8) -> Self {
        match val {
            0..=3 => unsafe { core::mem::transmute::<u8, Self>(val) },
            _ => Self::Mask,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum MemoryPoolType {
    Application = 0,
    Applet = 1,
    System = 2,
    SystemNonSecure = 3,
    #[default]
    Mask = 0xF,
}

impl MemoryPoolType {
    const fn into_bits(self) -> u8 {
        self as _
    }

    const fn from_bits(val: u8) -> Self {
        match val {
            0..=3 => unsafe { core::mem::transmute::<u8, Self>(val) },
            _ => Self::Mask,
        }
    }
}

#[bitfield_struct::bitfield(u32, order = Lsb)]
pub struct CreateProcessFlags {
    pub is_64bit: bool,
    #[bits(3, default = AddressSpaceType::Mask)]
    pub address_space_flags: AddressSpaceType,
    pub enable_debug: bool,
    pub enable_aslr: bool,
    pub is_application: bool,
    #[bits(4, default = MemoryPoolType::Mask)]
    pub memory_pool_type: MemoryPoolType,
    pub optimise_memory_allocation: bool,
    pub disable_device_address_space_merge: bool,
    pub alias_region_extra_size: bool,
    #[bits(18)]
    _unused: u32,
}

impl CreateProcessFlags {
    pub const fn all() -> Self {
        Self::new()
            .with_is_64bit(true)
            .with_address_space_flags(AddressSpaceType::Mask)
            .with_enable_debug(true)
            .with_enable_aslr(true)
            .with_is_application(true)
            .with_memory_pool_type(MemoryPoolType::Mask)
            .with_optimise_memory_allocation(true)
            .with_disable_device_address_space_merge(true)
            .with_alias_region_extra_size(true)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct CreateProcessInfo {
    pub name: ArrayString<12>,
    pub version: u32,
    pub program_id: u64,
    pub code_address: usize,
    pub code_num_pages: i32,
    pub flags: u32,
    pub resource_limit_handle: Handle,
    pub system_resource_page_count: i32,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub enum DebugThreadParam {
    ActualPriority = 0,
    State = 1,
    IdealCore = 2,
    CurrentCore = 3,
    CoreMask = 4,
}
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct PhysicalMemoryInfo {
    physical_address: usize,
    virtual_address: usize,
    size: usize,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct AttachProcessDebugEventInfo {
    pub program_id: ncm::ProgramId,
    pub process_id: u64,
    pub name: util::ArrayString<12>,
    pub flags: u32,
    pub user_exception_context_address: u64,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct AttachThreadDebugEventInfo {
    pub thread_id: u64,
    pub tls_ptr: usize,
    pub entrypoint: usize,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ExitDebugEventInfo {
    pub exit_type: u32,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ExceptionDebugEventInfo {
    pub exception_type: u32,
    pub fault_register: u32,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub union DebugEventInfo {
    pub attach_process: AttachProcessDebugEventInfo,
    pub attach_thread: AttachThreadDebugEventInfo,
    pub exit_process: ExitDebugEventInfo,
    pub exit_thread: ExitDebugEventInfo,
    pub exception: ExceptionDebugEventInfo,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum DebugEventType {
    AttachProcess,
    AttachThread,
    ExitProcess,
    ExitThread,
    Exception,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct DebugEvent {
    pub event_type: DebugEventType,
    pub flags: u32,
    pub thread_id: u32,
    pub info: DebugEventInfo,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ExceptionType {
    Init = 0x000,
    InstructionAbort = 0x100,
    DataAbort = 0x101,
    UnalignedInstruction = 0x102,
    UnalignedData = 0x103,
    UndefinedInstruction = 0x104,
    ExceptionInstruction = 0x105,
    MemorySystemError = 0x106,
    FpuException = 0x200,
    InvalidSystemCall = 0x301,
    SystemCallBreak = 0x302,
    AtmosphereStdAbort = 0xFFE,
}

pub type PageInfo = u32;
pub type Address = *const u8;
pub type MutAddress = *mut u8;
pub type Size = usize;
pub type ThreadEntrypointFn = unsafe extern "C" fn(*mut u8) -> !;
pub type Handle = u32;

pub struct ScopedHandle(pub Handle);
impl ScopedHandle {
    /// Creates a scoped guard for the handle.
    /// The handle can still be accessed and copied, but will become invalid when this struct is dropped.
    pub fn guard(handle: Handle) -> Self {
        Self(handle)
    }

    // Take the value out without running the destructor and closing the handle, consuming the guard
    pub unsafe fn take(guard: Self) -> Handle {
        mem::ManuallyDrop::new(guard).0
    }
}

impl Drop for ScopedHandle {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE {
            // ignore the error as it will only happen if the handle has already become invalid.
            let _ = close_handle(self.0);
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
/// Context of a scheduled thread.
pub struct LastThreadContext {
    /// Frame Pointer for the thread.
    fp: u64,
    /// Stack Pointer for the thread.
    sp: u64,
    /// Link Register for the thread.
    lr: u64,
    /// Program Counter for the thread.
    pc: u64,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
/// Limitable Resources.
pub enum LimitableResource {
    /// How much memory can a process map.
    Memory = 0,
    /// How many threads can a process spawn.
    Threads = 1,
    /// How many events can a process have.
    Events = 2,
    /// How many transfer memories can a process make.
    TransferMemories = 3,
    /// How many sessions can a process own.
    Sessions = 4,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
/// Thread/Process Scheduler State.
pub enum SchedulerState {
    /// Can be scheduled.
    Runnable = 0,
    /// Will not be scheduled.
    Paused = 1,
}

pub const INVALID_HANDLE: Handle = 0;

pub const CURRENT_THREAD_PSEUDO_HANDLE: Handle = 0xFFFF8000;
pub const CURRENT_PROCESS_PSEUDO_HANDLE: Handle = 0xFFFF8001;

pub const DEFAULT_PROCESS_PROCESSOR_ID: i32 = -2;

/// Set the process heap to a given size. It can both extend and shrink the heap.
#[inline(always)]
pub fn set_heap_size(size: Size) -> Result<MutAddress> {
    unsafe {
        let mut address: MutAddress = ptr::null_mut();

        let rc = asm::set_heap_size(&mut address, size);
        pack(rc, address)
    }
}

/// Set the memory permissions of a (page-aligned) range of memory.
///
/// `MemoryPermission::Execute()` and `MemoryPermission::Execute()` are not allowed.
/// This can be used to move back and forth between `MemoryPermission::None()`, `MemoryPermission::Read()`
/// and `MemoryPermission::Read() | MemoryPermission::Write()`.
#[inline(always)]
pub unsafe fn set_memory_permission(
    address: Address,
    size: Size,
    value: MemoryPermission,
) -> Result<()> {
    unsafe {
        let rc = asm::set_memory_permission(address, size, value);
        pack(rc, ())
    }
}

/// Set the memory attributes of a (page-aligned) range of memory.
///
/// Only setting or unsetting the `Uncached` flag (bit 3) is supported,
/// so the function signature has been changed from libnx to enforce this constraint.
///
/// # Safety
///
/// The provided address must be valid, and should be page aligned (0x1000).
#[inline(always)]
pub unsafe fn set_memory_attribute(address: Address, size: Size, set_uncached: bool) -> Result<()> {
    unsafe {
        let rc = asm::set_memory_attribute(
            address,
            size,
            8,
            if set_uncached {
                MemoryAttribute::Uncached()
            } else {
                MemoryAttribute::None()
            },
        );
        pack(rc, ())
    }
}

/// Maps a memory range into a different range. Mainly used for adding guard pages around stack.
///
/// Source range gets reprotected to [`MemoryAttribute::None()`] (it can no longer be accessed),
/// and [`MemoryAttribute::Borrowed()`] is set in the source page's [`MemoryAttribute`].
#[inline(always)]
pub unsafe fn map_memory(address: Address, source_address: MutAddress, size: Size) -> Result<()> {
    unsafe {
        let rc = asm::map_memory(address, source_address, size);
        pack(rc, ())
    }
}

/// Unmaps a region that was previously mapped with [`map_memory`]
#[inline(always)]
pub unsafe fn unmap_memory(address: Address, source_address: MutAddress, size: Size) -> Result<()> {
    unsafe {
        let rc = asm::unmap_memory(address, source_address, size);
        pack(rc, ())
    }
}

/// Query information about an address. Will always fetch the lowest page-aligned mapping that contains the provided address.
///
/// null pointers are OK here, as we are just querying memory properties.
#[inline(always)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn query_memory(address: Address) -> Result<(MemoryInfo, PageInfo)> {
    unsafe {
        let mut memory_info: MemoryInfo = Default::default();
        let mut page_info: PageInfo = 0;

        let rc = asm::query_memory(&mut memory_info, &mut page_info, address);
        pack(rc, (memory_info, page_info))
    }
}

/// What it says on the tin.
#[inline(always)]
pub fn exit_process() -> ! {
    unsafe { asm::exit_process() }
}

/// Creates a thread.
///
/// The pointer to the thread arguments and stack memory _must_ live at least as long as the thread is alive.
#[inline(always)]
pub unsafe fn create_thread(
    entry: ThreadEntrypointFn,
    entry_arg: MutAddress,
    stack_top: MutAddress,
    priority: i32,
    processor_id: i32,
) -> Result<Handle> {
    unsafe {
        let mut handle: Handle = 0;

        let rc = asm::create_thread(
            &mut handle,
            entry,
            entry_arg,
            stack_top,
            priority,
            processor_id,
        );
        pack(rc, handle)
    }
}

/// Starts executing a prepared thread by handle (received from [`create_thread`]).
#[inline(always)]
pub fn start_thread(handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::start_thread(handle);
        pack(rc, ())
    }
}

/// What it says on the tin.
#[inline(always)]
pub fn exit_thread() -> ! {
    unsafe { asm::exit_thread() }
}

/// Sleep the current thread for at least `timeout` nanoseconds or yields if passed a value from [`YieldType`][`crate::thread::YieldType`]
#[inline(always)]
pub fn sleep_thread(timeout: i64) -> Result<()> {
    unsafe {
        let rc = asm::sleep_thread(timeout);
        pack(rc, ())
    }
}

/// Gets a thread's priority.
#[inline(always)]
pub fn get_thread_priority(handle: Handle) -> Result<i32> {
    unsafe {
        let mut priority: i32 = 0;

        let rc = asm::get_thread_priority(&mut priority, handle);
        pack(rc, priority)
    }
}

/// Sets a thread's priority.
#[inline(always)]
pub fn set_thread_priority(handle: Handle, priority: i32) -> Result<()> {
    unsafe {
        let rc = asm::set_thread_priority(handle, priority);
        pack(rc, ())
    }
}

/// Gets a thread's core mask.
#[inline(always)]
pub fn get_thread_core_mask(handle: Handle) -> Result<(i32, u64)> {
    unsafe {
        let mut mask = 0;
        let mut affinity = 0;
        let rc = asm::get_thread_core_mask(&mut mask, &mut affinity, handle);
        pack(rc, (mask, affinity))
    }
}

/// Sets a thread's core mask.
#[inline(always)]
pub fn set_thread_core_mask(handle: Handle, preferred_core: i32, affinity_mask: u32) -> Result<()> {
    unsafe {
        let rc = asm::set_thread_core_mask(handle, preferred_core, affinity_mask);
        pack(rc, ())
    }
}

/// Gets the processor number (core) that the current thread is executing on.
#[inline(always)]
pub fn get_current_processor_number() -> u32 {
    unsafe { asm::get_current_processor_number() }
}

// Sets an event's signalled status.
#[inline(always)]
pub fn signal_event(handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::signal_event(handle);
        pack(rc, ())
    }
}

// Clears an event's signalled status.
#[inline(always)]
pub fn clear_event(handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::clear_event(handle);
        pack(rc, ())
    }
}

/// Maps a block of shared memory.
#[inline(always)]
pub unsafe fn map_shared_memory(
    handle: Handle,
    address: MutAddress,
    size: Size,
    permission: MemoryPermission,
) -> Result<()> {
    unsafe {
        let rc = asm::map_shared_memory(handle, address, size, permission);
        pack(rc, ())
    }
}

/// Unmaps a block of shared memory.
#[inline(always)]
pub unsafe fn unmap_shared_memory(handle: Handle, address: Address, size: Size) -> Result<()> {
    unsafe {
        let rc = asm::unmap_shared_memory(handle, address, size);
        pack(rc, ())
    }
}

/// Creates a block of transfer memory.
///
/// The memory will be reprotected with `permissions` after creation (usually set to none).
/// The original memory permissions will be restored when the handle is closed.
#[inline(always)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn create_transfer_memory(
    address: MutAddress,
    size: Size,
    permissions: MemoryPermission,
) -> Result<Handle> {
    unsafe {
        let mut handle: Handle = 0;

        let rc = asm::create_transfer_memory(&mut handle, address, size, permissions);
        pack(rc, handle)
    }
}

/// Closes a handle, decrementing the reference count of the corresponding kernel object.
#[inline(always)]
pub fn close_handle(handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::close_handle(handle);
        pack(rc, ())
    }
}

/// Resets a signal.
#[inline(always)]
pub fn reset_signal(handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::reset_signal(handle);
        pack(rc, ())
    }
}

/// Waits on one or more synchronization objects, optionally with a timeout.
///
/// The max number of handles is `0x40` (64). This is a Horizon kernel limitation.
#[inline(always)]
pub unsafe fn wait_synchronization(handles: &[Handle], timeout: i64) -> Result<i32> {
    unsafe {
        let mut index: i32 = 0;
        let rc =
            asm::wait_synchronization(&mut index, handles.as_ptr(), handles.len() as u32, timeout);
        pack(rc, index)
    }
}

/// The same as [`wait_synchronization`] for a single handle
#[inline(always)]
pub fn wait_synchronization_one(handle: Handle, timeout: i64) -> Result<()> {
    unsafe {
        let mut index: i32 = 0;
        let rc = asm::wait_synchronization(&mut index, &handle, 1u32, timeout);
        pack(rc, ())
    }
}

/// If the referenced thread is currently in a synchronization call ([`wait_synchronization`], [`reply_and_receive`]
/// or [`reply_and_receive_light`]), that call will be interrupted and return `0xec01`([`ResultCancelled`][`rc::ResultCancelled`]) .
/// If that thread is not currently executing such a synchronization call, the next call to a synchronization call will return `0xec01``.
///
/// This doesn't take force-pause (activity/debug pause) into account.
#[inline(always)]
pub fn cancel_synchronization(thread_handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::cancel_synchronization(thread_handle);
        pack(rc, ())
    }
}

/// Arbitrates a mutex lock operation in userspace.
#[inline(always)]
pub unsafe fn arbitrate_lock(thread_handle: Handle, tag_location: Address, tag: u32) -> Result<()> {
    unsafe {
        let rc = asm::arbitrate_lock(thread_handle, tag_location, tag);
        pack(rc, ())
    }
}

/// Arbitrates a mutex unlock operation in userspace.
#[inline(always)]
pub unsafe fn arbitrate_unlock(tag_location: Address) -> Result<()> {
    unsafe {
        let rc = asm::arbitrate_unlock(tag_location);
        pack(rc, ())
    }
}

/// Performs a condition variable wait operation in userspace.
#[inline(always)]
pub unsafe fn wait_process_wide_key_atomic(
    wait_location: Address,
    tag_location: Address,
    desired_tag: u32,
    timeout: i64,
) -> Result<()> {
    unsafe {
        let rc =
            asm::wait_process_wide_key_atomic(wait_location, tag_location, desired_tag, timeout);
        pack(rc, ())
    }
}

/// Performs a condition variable wake-up operation in userspace.
#[inline(always)]
pub unsafe fn signal_process_wide_key(tag_location: Address, desired_tag: i32) -> Result<()> {
    unsafe {
        let rc = asm::signal_process_wide_key(tag_location, desired_tag);
        pack(rc, ())
    }
}

/// Gets the current system tick.
#[inline(always)]
pub fn get_system_tick() -> u64 {
    unsafe { asm::get_system_tick() }
}

/// Connects to a registered named port.
#[inline(always)]
pub unsafe fn connect_to_named_port(name: &core::ffi::CStr) -> Result<Handle> {
    unsafe {
        let mut handle: Handle = 0;

        let rc = asm::connect_to_named_port(&mut handle, name.as_ptr().cast());
        pack(rc, handle)
    }
}

/// Sends a light IPC synchronization request to a session.
#[inline(always)]
pub fn send_sync_request_light(handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::send_sync_request_light(handle);
        pack(rc, ())
    }
}

/// Sends a synchronous IPC request to a session.
#[inline(always)]
pub fn send_sync_request(handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::send_sync_request(handle);
        pack(rc, ())
    }
}

/// Sends a synchronous IPC request to a session from an user allocated buffer.
///
/// The buffer size must be a multiple of the system page size (0x1000).
#[inline(always)]
pub unsafe fn send_sync_request_with_user_data(buffer: &mut [u8], handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::send_sync_request_with_user_data(buffer.as_mut_ptr(), buffer.len(), handle);
        pack(rc, ())
    }
}

/// Sends an asynchronous IPC request to a session from an user allocated buffer .
///
/// The buffer size must be a multiple of the system page size (0x1000).
#[inline(always)]
pub unsafe fn send_async_request_with_user_data(
    buffer: &mut [u8],
    session: Handle,
) -> Result<Handle> {
    unsafe {
        let mut out_handle = 0;
        let rc = asm::send_async_request_with_user_data(
            &mut out_handle,
            buffer.as_mut_ptr(),
            buffer.len(),
            session,
        );
        pack(rc, out_handle)
    }
}

/// Gets the PID associated with a process.
#[inline(always)]
pub fn get_process_id(process_handle: Handle) -> Result<u64> {
    unsafe {
        let mut process_id: u64 = 0;

        let rc = asm::get_process_id(&mut process_id, process_handle);
        pack(rc, process_id)
    }
}

/// Gets the TID associated with a process.
#[inline(always)]
pub fn get_thread_id(handle: Handle) -> Result<u64> {
    unsafe {
        let mut thread_id: u64 = 0;

        let rc = asm::get_thread_id(&mut thread_id, handle);
        pack(rc, thread_id)
    }
}

/// Breaks execution
///
/// The `debug_data` buffer is passed to a debugging instance if one is attached.
#[inline(always)]
pub fn r#break(reason: BreakReason, debug_data: &[u8]) -> Result<()> {
    unsafe {
        let rc = asm::r#break(reason, debug_data.as_ptr(), debug_data.len());
        pack(rc, ())
    }
}

/// Outputs debug text, if used during debugging.
#[inline(always)]
pub unsafe fn output_debug_string(msg: &core::ffi::CStr) -> Result<()> {
    unsafe {
        let rc = asm::output_debug_string(msg.as_ptr().cast(), msg.count_bytes());
        pack(rc, ())
    }
}

/// Returns from an exception.
#[inline(always)]
pub fn return_from_exception(res: ResultCode) -> ! {
    unsafe { asm::return_from_exception(res) }
}

/// Retrieves information about the system, or a certain kernel object, depending on the value of `id`.
///
/// `handle` is for particular kernel objects, but `INVALID_HANDLE` is used to retrieve information about the system.
#[inline(always)]
pub fn get_info(id: InfoId, handle: Handle, sub_id: u64) -> Result<u64> {
    unsafe {
        let mut info: u64 = 0;

        let rc = asm::get_info(&mut info, id, handle, sub_id);
        pack(rc, info)
    }
}

/*
/// Flushes the entire data cache (by set/way).
///
/// This is a privileged syscall and may not be available.
///
/// This syscall has dangerous side effects and should not be used.
#[inline(always)]
#[deprecated]
pub unsafe fn flush_entire_data_cache() -> Result<()> {
    unsafe {
        let rc = asm::flush_entire_data_cache();
        pack(rc, ())
    }
}

    */

/// Flushes data cache for a virtual address range.
///
/// [`cache_flush`][`crate::arm::cache_flush`] should be used instead whenever possible.
#[inline(always)]
#[deprecated]
pub unsafe fn flush_data_cache(address: Address, len: Size) -> Result<()> {
    unsafe {
        let rc = asm::flush_data_cache(address, len);
        pack(rc, ())
    }
}

/// Maps new heap memory at the desired address. [3.0.0+]
#[inline(always)]
pub unsafe fn map_physical_memory(address: Address, len: Size) -> Result<()> {
    unsafe {
        let rc = asm::map_physical_memory(address, len);
        pack(rc, ())
    }
}

/// Unmaps memory mapped by [`map_physical_memory`].  [3.0.0+]
#[inline(always)]
pub unsafe fn unmap_physical_memory(address: Address, len: Size) -> Result<()> {
    unsafe {
        let rc = asm::unmap_physical_memory(address, len);
        pack(rc, ())
    }
}

/// Gets information about a thread that will be scheduled in the future. [5.0.0+]
///
/// `ns` is the nanoseconds in the future when the thread information will be sampled
/// by the kernel.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn get_debug_future_thread_info(
    debug_proc_handle: Handle,
    ns: i64,
) -> Result<(LastThreadContext, u64)> {
    unsafe {
        let mut out_context = mem::zeroed();
        let mut out_thread_id = 0;
        let rc = asm::get_debug_future_thread_info(
            &mut out_context,
            &mut out_thread_id,
            debug_proc_handle,
            ns,
        );
        pack(rc, (out_context, out_thread_id))
    }
}

/// Gets information about the previously-scheduled thread.
#[inline(always)]
pub fn get_last_thread_info() -> Result<(LastThreadContext, u64, u32)> {
    unsafe {
        let mut out_context = mem::zeroed();
        let mut out_tls_address = 0;
        let mut out_flags = 0;
        let rc = asm::get_last_thread_info(&mut out_context, &mut out_tls_address, &mut out_flags);
        pack(rc, (out_context, out_tls_address, out_flags))
    }
}

/// Gets the maximum value a LimitableResource can have, for a Resource Limit handle.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn get_resource_limit_limit_value(
    resource_limit_handle: Handle,
    limit_kind: LimitableResource,
) -> Result<i64> {
    unsafe {
        let mut out_val = 0;
        let rc =
            asm::get_resource_limit_limit_value(&mut out_val, resource_limit_handle, limit_kind);
        pack(rc, out_val)
    }
}

/// Gets the current value a LimitableResource has, for a Resource Limit handle.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn get_resource_limit_current_value(
    resource_limit_handle: Handle,
    limit_kind: LimitableResource,
) -> Result<i64> {
    unsafe {
        let mut out_val = 0;
        let rc =
            asm::get_resource_limit_current_value(&mut out_val, resource_limit_handle, limit_kind);
        pack(rc, out_val)
    }
}

/// Pauses/unpauses a thread.
#[inline(always)]
pub fn set_thread_activity(thread_handle: Handle, thread_state: SchedulerState) -> Result<()> {
    unsafe {
        let rc = asm::set_thread_activity(thread_handle, thread_state);
        pack(rc, ())
    }
}

/// Dumps the registers of a thread paused by [`set_thread_activity`] (register groups: all)
#[inline(always)]
pub fn get_thread_context3(thread_handle: Handle) -> Result<()> {
    unsafe {
        let mut out_context = Default::default();
        let rc = asm::get_thread_context3(&mut out_context, thread_handle);
        pack(rc, ())
    }
}

/// Arbitrates an address depending on type and value. [4.0.0+]
#[inline(always)]
pub unsafe fn wait_for_address(
    address: Address,
    arbitration_type: ArbitrationType,
    value: u32,
    timeout: i64,
) -> Result<()> {
    unsafe {
        let rc = asm::wait_for_address(address, arbitration_type as u32, value, timeout);
        pack(rc, ())
    }
}

/// Signals (and updates) an address depending on type and value. [4.0.0+]
#[inline(always)]
pub unsafe fn signal_to_address(
    address: Address,
    signal: SignalType,
    value: u32,
    thread_signal_count: i32,
) -> Result<()> {
    unsafe {
        let rc = asm::signal_to_address(address, signal as u32, value, thread_signal_count);
        pack(rc, ())
    }
}

/// Sets thread preemption state (used during abort/panic). [8.0.0+]
#[inline(always)]
pub unsafe fn synchronize_preemption_state() -> Result<()> {
    unsafe {
        let rc = asm::synchronize_preemption_states();
        pack(rc, ())
    }
}

/// Creates an IPC session.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn create_session(is_light: bool, unk_name: u64) -> Result<(Handle, Handle)> {
    unsafe {
        let mut server_handle: Handle = 0;
        let mut client_handle: Handle = 0;

        let rc = asm::create_session(&mut server_handle, &mut client_handle, is_light, unk_name);
        pack(rc, (server_handle, client_handle))
    }
}

/// Accepts an IPC session.
#[inline(always)]
pub fn accept_session(handle: Handle) -> Result<Handle> {
    unsafe {
        let mut session_handle: Handle = 0;

        let rc = asm::accept_session(&mut session_handle, handle);
        pack(rc, session_handle)
    }
}

/// Performs light IPC input/output.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn reply_and_receive_light(handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::reply_and_receive_light(handle);
        pack(rc, ())
    }
}

/// Performs IPC input/output.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub unsafe fn reply_and_receive(
    handles: *const Handle,
    handle_count: u32,
    reply_target: Handle,
    timeout: i64,
) -> Result<i32> {
    unsafe {
        let mut index: i32 = 0;

        let rc = asm::reply_and_receive(&mut index, handles, handle_count, reply_target, timeout);
        pack(rc, index)
    }
}

/// Performs IPC input/output on a user-allocated buffer.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub unsafe fn reply_and_receive_with_user_buffer(
    user_buffer: &mut [u8],
    handles: &[Handle],
    reply_target: Handle,
    timeout: i64,
) -> Result<i32> {
    unsafe {
        let mut index: i32 = 0;

        let rc = asm::reply_and_receive_with_user_buffer(
            &mut index,
            user_buffer.as_mut_ptr(),
            user_buffer.len(),
            handles.as_ptr(),
            handles.len() as u32,
            reply_target,
            timeout,
        );
        pack(rc, index)
    }
}

/// Creates a system event.
#[inline(always)]
pub fn create_event() -> Result<(Handle, Handle)> {
    unsafe {
        let mut server_handle: Handle = 0;
        let mut client_handle: Handle = 0;

        let rc = asm::create_event(&mut server_handle, &mut client_handle);
        pack(rc, (server_handle, client_handle))
    }
}

/// Debugs an active process.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn debug_active_process(process_id: u64) -> Result<Handle> {
    unsafe {
        let mut handle: Handle = 0;
        let rc = asm::debug_active_process(&mut handle, process_id);
        pack(rc, handle)
    }
}

/// Breaks an active debugging session.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn break_debug_process(debug_handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::break_debug_process(debug_handle);
        pack(rc, ())
    }
}

/// Terminates the process of an active debugging session
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn terminate_debug_process(debug_handle: Handle) -> Result<()> {
    unsafe {
        let rc = asm::terminate_debug_process(debug_handle);
        pack(rc, ())
    }
}

/// Gets an incoming debug event from a debugging session.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn get_debug_event(debug_handle: Handle) -> Result<DebugEvent> {
    unsafe {
        let mut debug_event: DebugEvent = mem::zeroed();

        let rc = asm::get_debug_event(&mut debug_event, debug_handle);
        pack(rc, debug_event)
    }
}

/// Continues a debugging session. [3.0.0+]
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn continue_debug_event(debug_handle: Handle, flags: u32, thread_ids: &[u64]) -> Result<()> {
    unsafe {
        let rc = asm::continue_debug_event(
            debug_handle,
            flags,
            thread_ids.as_ptr(),
            thread_ids.len() as u32,
        );
        pack(rc, ())
    }
}

///Retrieves a list of all running processes. Returns the number of PIDs written to the buffer.
#[inline(always)]
pub fn get_process_list(process_id_list: &mut [u64]) -> Result<usize> {
    unsafe {
        let mut count: u32 = 0;

        let rc = asm::get_process_list(
            &mut count,
            process_id_list.as_mut_ptr(),
            process_id_list.len() as u32,
        );
        pack(rc, count as usize)
    }
}

/// Retrieves a list of all threads for a debug handle (or zero). Returns the number of thread IDs written to the buffer.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn get_thread_list(debug_handle: Handle, thread_id_list: &mut [u64]) -> Result<usize> {
    unsafe {
        let mut count: u32 = 0;

        let rc = asm::get_thread_list(
            &mut count,
            thread_id_list.as_mut_ptr(),
            thread_id_list.len() as u32,
            debug_handle,
        );
        pack(rc, count as usize)
    }
}

/// Queries the thread context for a thread under debugging.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn get_debug_thread_context(
    debug_handle: Handle,
    thread_id: u64,
    register_group: arm::RegisterGroup,
) -> Result<arm::ThreadContext> {
    unsafe {
        let mut thread_context: arm::ThreadContext = Default::default();

        let rc = asm::get_debug_thread_context(
            &raw mut thread_context,
            debug_handle,
            thread_id,
            register_group.get(),
        );
        pack(rc, thread_context)
    }
}

/// Writes the thread context (scoped by `register_group`) back into a thread under debugging.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn set_debug_thread_context(
    debug_handle: Handle,
    thread_context: arm::ThreadContext,
    thread_id: u64,
    register_group: arm::RegisterGroup,
) -> Result<()> {
    unsafe {
        let rc = asm::set_debug_thread_context(
            debug_handle,
            thread_id,
            &raw const thread_context,
            register_group.get(),
        );
        pack(rc, ())
    }
}

/// Gets the memory metadata for an address in a process under debugging.
///
/// This is a privileged syscall and may not be available.
///
/// # Safety
///
/// null pointers are OK here, as we are just querying the memory's information
#[inline(always)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn query_debug_process_memory(
    debug_handle: Handle,
    address: usize,
) -> Result<(MemoryInfo, PageInfo)> {
    unsafe {
        let mut memory_info: MemoryInfo = Default::default();
        let mut page_info: PageInfo = 0;

        let rc = asm::query_debug_process_memory(
            &mut memory_info,
            &mut page_info,
            debug_handle,
            address,
        );
        pack(rc, (memory_info, page_info))
    }
}

/// Reads memory from an address in a process under debugging.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub unsafe fn read_debug_process_memory(
    debug_handle: Handle,
    read_address: usize,
    buffer: &mut [u8],
) -> Result<()> {
    unsafe {
        let rc = asm::read_debug_process_memory(
            buffer.as_mut_ptr(),
            debug_handle,
            read_address,
            buffer.len(),
        );
        pack(rc, ())
    }
}

/// Reads memory to an address in a process under debugging.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub unsafe fn write_debug_process_memory(
    debug_handle: Handle,
    write_address: usize,
    write_size: usize,
    buffer: *const u8,
) -> Result<()> {
    unsafe {
        let rc = asm::write_debug_process_memory(debug_handle, buffer, write_address, write_size);
        pack(rc, ())
    }
}

/// Creates a named port.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub unsafe fn create_named_port(
    name: &core::ffi::CStr,
    max_sessions: i32,
    is_light: bool,
) -> Result<(Handle, Handle)> {
    unsafe {
        let mut server_handle = 0;
        let mut client_handle = 0;

        let rc = asm::create_named_port(
            &mut server_handle,
            &mut client_handle,
            max_sessions,
            is_light,
            name.as_ptr().cast(),
        );
        pack(rc, (server_handle, client_handle))
    }
}

/// Manages a named port.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub unsafe fn manage_named_port(name: &core::ffi::CStr, max_sessions: i32) -> Result<Handle> {
    unsafe {
        let mut handle: Handle = 0;

        let rc = asm::manage_named_port(&mut handle, name.as_ptr().cast(), max_sessions);
        pack(rc, handle)
    }
}

/// Connects a named port.
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub unsafe fn connect_named_port(client_session: Handle) -> Result<Handle> {
    unsafe {
        let mut session_handle: Handle = 0;

        let rc = asm::connect_to_port(&mut session_handle, client_session);
        pack(rc, session_handle)
    }
}

/// Calls a secure monitor function (TrustZone, EL3).
///
/// This is a privileged syscall and may not be available.
#[inline(always)]
pub fn call_secure_monitor(mut secmon_args: [u64; 8]) -> [u64; 8] {
    unsafe {
        asm::call_secure_monitor(secmon_args.as_mut_ptr());
    }
    secmon_args
}
