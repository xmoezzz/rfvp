use core::{marker::PhantomData, mem::MaybeUninit};

use super::*;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};

pub use nx_derive::{Request, Response, ipc_trait};
use zeroize::Zeroize;

pub struct Buffer<
    'borrow,
    const IN: bool,
    const OUT: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
> {
    buf: *mut T,
    count: usize,
    _lifetime: PhantomData<&'borrow ()>,
}

impl<
    'borrow,
    const IN: bool,
    const OUT: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
>
    Buffer<
        'borrow,
        IN,
        OUT,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        u8,
    >
{
    pub const fn from_other_mut_var<'a: 'borrow, U>(var: &'a mut U) -> Self {
        unsafe { Self::from_ptr::<'a>(var as *const U as *const u8, size_of::<U>()) }
    }
}

impl<
    'borrow,
    const IN: bool,
    const OUT: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
>
    Buffer<
        'borrow,
        IN,
        OUT,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        T,
    >
{
    // TODO: ensure that sizeof(T) is a multiple of size

    /// Creates a `Buffer` from raw parts
    ///
    /// # Safety
    ///
    /// It is the caller's responsibility to ensure the lifetime of the buffer does not exceed the
    /// inner data.
    pub const unsafe fn new<'a: 'borrow>(addr: *mut u8, size: usize) -> Self {
        Self {
            buf: addr as *mut T,
            count: size / core::mem::size_of::<T>(),
            _lifetime: PhantomData,
        }
    }

    /// Creates a `Buffer` from a raw pointer
    ///
    /// # Safety
    ///
    /// It is the caller's responsibility to ensure the lifetime of the buffer does not exceed the
    /// inner data.
    pub const unsafe fn from_ptr<'a: 'borrow>(buf: *const T, count: usize) -> Self {
        Self {
            buf: buf as *mut T,
            count,
            _lifetime: PhantomData,
        }
    }

    /// Creates a `Buffer` from a raw pointer
    ///
    /// # Safety
    ///
    /// It is the caller's responsibility to ensure the lifetime of the buffer does not exceed the
    /// inner data.
    pub const unsafe fn from_mut_ptr<'a: 'borrow>(buf: *mut T, count: usize) -> Self {
        Self {
            buf,
            count,
            _lifetime: PhantomData,
        }
    }

    /// Converts a Buffer from one flag set to another
    ///
    /// # Arguments:
    ///
    /// * `other`: The other buffer to clone
    ///
    /// # Safety
    ///
    /// Since this clones the raw pointer, this can be used to get 2 mutable references to the same data.
    /// The caller _MUST_ ensure that only one the passed `other` buffer or the produced buffer is ever
    /// read/written while the other is alive.
    pub const unsafe fn from_other<
        'other: 'borrow,
        const IN2: bool,
        const OUT2: bool,
        const MAP_ALIAS2: bool,
        const POINTER2: bool,
        const FIXED_SIZE2: bool,
        const AUTO_SELECT2: bool,
        const ALLOW_NON_SECURE2: bool,
        const ALLOW_NON_DEVICE2: bool,
        U,
    >(
        other: &'other Buffer<
            IN2,
            OUT2,
            MAP_ALIAS2,
            POINTER2,
            FIXED_SIZE2,
            AUTO_SELECT2,
            ALLOW_NON_SECURE2,
            ALLOW_NON_DEVICE2,
            U,
        >,
    ) -> Self {
        unsafe { Self::new(other.get_address(), other.get_size()) }
    }

    pub const fn get_address(&self) -> *mut u8 {
        self.buf.cast()
    }

    pub const fn get_size(&self) -> usize {
        self.count * core::mem::size_of::<T>()
    }

    pub const fn get_count(&self) -> usize {
        self.count
    }

    pub fn get_mut_var(&mut self) -> &mut T {
        unsafe { self.buf.as_mut().unwrap() }
    }

    pub fn set_var(&mut self, t: T) {
        unsafe {
            *self.buf.as_mut().unwrap() = t;
        }
    }

    pub fn get_maybe_unaligned(&self) -> Vec<T> {
        assert!(!self.buf.is_null());
        let mut out = Vec::with_capacity(self.count);
        for index in 0..self.count {
            // SAFETY: we have already asserted on non-null `self.buf`
            out.push(unsafe { core::ptr::read_unaligned(self.buf.add(index)) });
        }

        out
    }
}

impl<
    'borrow,
    const IN: bool,
    const OUT: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
>
    Buffer<
        'borrow,
        IN,
        OUT,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        T,
    >
{
    pub const fn from_mut_var(var: &'borrow mut T) -> Self {
        unsafe { Self::from_mut_ptr::<'borrow>(var as *mut T, 1) }
    }

    pub const fn from_mut_array(arr: &'borrow mut [T]) -> Self {
        unsafe { Self::from_mut_ptr(arr.as_mut_ptr(), arr.len()) }
    }
}

impl<
    'borrow,
    const OUT: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
>
    Buffer<
        'borrow,
        true,
        OUT,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        T,
    >
{
    pub fn get_var(&self) -> Result<&T> {
        unsafe {
            self.buf
                .as_ref()
                .ok_or(rc::ResultInvalidBufferPointer::make())
        }
    }

    pub fn as_slice(&self) -> Result<&[T]> {
        result_return_unless!(
            self.buf.is_aligned() && !self.buf.is_null(),
            rc::ResultInvalidBufferPointer
        );
        Ok(unsafe { core::slice::from_raw_parts(self.buf, self.count) })
    }
}

impl<
    'borrow,
    const IN: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
>
    Buffer<
        'borrow,
        IN,
        false,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        u8,
    >
{
    pub const fn from_other_var<'a: 'borrow, U>(var: &'a U) -> Self {
        unsafe { Self::from_ptr::<'a>(var as *const U as *const _, size_of::<U>()) }
    }
}

impl<
    'borrow,
    const IN: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
>
    Buffer<
        'borrow,
        IN,
        false,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        T,
    >
{
    pub const fn from_var(var: &'borrow T) -> Self {
        unsafe { Self::from_ptr(var as *const T, 1) }
    }

    pub const fn from_array(arr: &'borrow [T]) -> Self {
        unsafe { Self::from_ptr(arr.as_ptr(), arr.len()) }
    }
}

impl<
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
>
    Buffer<
        '_,
        true,
        true,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        T,
    >
{
    pub fn as_slice_mut(&mut self) -> Result<&mut [T]> {
        result_return_unless!(
            self.buf.is_aligned() && !self.buf.is_null(),
            rc::ResultInvalidBufferPointer
        );
        Ok(unsafe { core::slice::from_raw_parts_mut(self.buf, self.count) })
    }
}

impl<
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
>
    Buffer<
        '_,
        false,
        true,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        T,
    >
{
    /// If the input buffer is not marked as "IN" then there isn't an API contract that it will be readable/initialized.
    /// As such, we should consider all "OUT + !IN" buffers as uninitialized until written by the server function.
    pub fn as_maybeuninit_mut(&mut self) -> Result<&mut [MaybeUninit<T>]> {
        result_return_unless!(
            self.buf.is_aligned() && !self.buf.is_null(),
            rc::ResultInvalidBufferPointer
        );
        Ok(unsafe { core::slice::from_raw_parts_mut(self.buf.cast(), self.count) })
    }
}

impl<
    const OUT: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
>
    Buffer<
        '_,
        true,
        OUT,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        u8,
    >
{
    pub fn get_string(&self) -> String {
        String::from_utf8_lossy(unsafe { core::slice::from_raw_parts_mut(self.buf, self.count) })
            .to_string()
    }
}

impl<
    const IN: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
>
    Buffer<
        '_,
        IN,
        true,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        u8,
    >
{
    pub fn set_string(&mut self, string: String) {
        unsafe {
            // First memset to zero so that it will be a valid nul-terminated string
            core::slice::from_raw_parts_mut(self.buf, self.count).zeroize();
            core::ptr::copy(
                string.as_ptr(),
                self.buf,
                core::cmp::min(self.count - 1, string.len()),
            );
        }
    }
}

pub type InMapAliasBuffer<'borrow, T> =
    Buffer<'borrow, true, false, true, false, false, false, false, false, T>;
pub type OutMapAliasBuffer<'borrow, T> =
    Buffer<'borrow, false, true, true, false, false, false, false, false, T>;
pub type InNonSecureMapAliasBuffer<'borrow, T> =
    Buffer<'borrow, true, false, true, false, false, false, true, false, T>;
pub type OutNonSecureMapAliasBuffer<'borrow, T> =
    Buffer<'borrow, false, true, true, false, false, false, true, false, T>;
pub type InAutoSelectBuffer<'borrow, T> =
    Buffer<'borrow, true, false, false, false, false, true, false, false, T>;
pub type OutAutoSelectBuffer<'borrow, T> =
    Buffer<'borrow, false, true, false, false, false, true, false, false, T>;
pub type InOutAutoSelectBuffer<'borrow, T> =
    Buffer<'borrow, true, true, false, false, false, true, false, false, T>;
pub type InPointerBuffer<'borrow, T> =
    Buffer<'borrow, true, false, false, true, false, false, false, false, T>;
pub type OutPointerBuffer<'borrow, T> =
    Buffer<'borrow, false, true, false, true, false, false, false, false, T>;
pub type InFixedPointerBuffer<'borrow, T> =
    Buffer<'borrow, true, false, false, true, true, false, false, false, T>;
pub type OutFixedPointerBuffer<'borrow, T> =
    Buffer<'borrow, false, true, false, true, true, false, false, false, T>;

#[derive(Clone)]
pub struct Handle<const MOVE: bool> {
    pub handle: svc::Handle,
}

impl<const MOVE: bool> Handle<MOVE> {
    pub const fn from(handle: svc::Handle) -> Self {
        Self { handle }
    }
}

pub type CopyHandle = Handle<false>;
pub type MoveHandle = Handle<true>;

#[derive(Clone, Default)]
pub struct ProcessId {
    pub process_id: u64,
}

impl ProcessId {
    pub const fn from(process_id: u64) -> Self {
        Self { process_id }
    }

    pub const fn new() -> Self {
        Self { process_id: 0 }
    }
}

/// AppletResourceUserIds are restricted to the values of zero, or the process' PID.
/// When they are sent over an IPC interface, they also trigger the sending of a PID descriptor in the HIPC request,
/// so there is an additional field for the PID. This field is filled in by the kernel during a request, and is read
/// out of the headers in the same way as the `ProcessId`[`ProcessId`] above.
///
/// This allows the crate to just send the `AppletResourceUserId` object when the IPC interface is expecting this value
/// and the `send_pid` flag. This also allows us to have a `ProcessId` type that creates it's own pid placeholder in CMIF
/// IPC requests.
#[derive(Clone, Default)]
pub struct AppletResourceUserId {
    pub process_id: u64,
    pub aruid: u64,
}

impl AppletResourceUserId {
    pub const fn from(process_id: u64, aruid: u64) -> Self {
        Self { process_id, aruid }
    }

    #[cfg(feature = "applet")]
    pub fn from_global() -> Self {
        Self {
            process_id: 0,
            aruid: crate::applet::GLOBAL_ARUID.load(core::sync::atomic::Ordering::SeqCst),
        }
    }

    pub const fn new(aruid: u64) -> Self {
        Self {
            process_id: 0,
            aruid,
        }
    }
}

// This is used, for instance, with u8-sized enums which are sent/received as u32s in commands

#[derive(Copy, Clone)]
#[repr(C)]
pub union EnumAsPrimitiveType<E: Copy + Clone, T: Copy + Clone> {
    val: T,
    enum_val: E,
}

impl<E: Copy + Clone, T: Copy + Clone> EnumAsPrimitiveType<E, T> {
    pub fn from(enum_val: E) -> Self {
        Self { enum_val }
    }

    pub fn from_val(val: T) -> Self {
        Self { val }
    }

    pub fn get(&self) -> E {
        unsafe { self.enum_val }
    }

    pub fn set(&mut self, enum_val: E) {
        self.enum_val = enum_val;
    }

    pub fn get_value(&self) -> T {
        unsafe { self.val }
    }

    pub fn set_value(&mut self, val: T) {
        self.val = val;
    }
}

impl<E: Copy + Clone, T: Copy + Clone>
    server::RequestCommandParameter<'_, EnumAsPrimitiveType<E, T>> for EnumAsPrimitiveType<E, T>
{
    fn after_request_read(ctx: &mut server::ServerContext) -> Result<Self> {
        Ok(ctx.raw_data_walker.advance_get())
    }
}

impl<E: Copy + Clone, T: Copy + Clone> server::ResponseCommandParameter
    for EnumAsPrimitiveType<E, T>
{
    type CarryState = ();
    fn before_response_write(_raw: &Self, ctx: &mut server::ServerContext) -> Result<()> {
        ctx.raw_data_walker.advance::<Self>();
        Ok(())
    }

    fn after_response_write(
        raw: Self,
        _carry_state: (),
        ctx: &mut server::ServerContext,
    ) -> Result<()> {
        ctx.raw_data_walker.advance_set(raw);
        Ok(())
    }
}

impl client::RequestCommandParameter for core::time::Duration {
    fn before_request_write(
        _var: &Self,
        walker: &mut DataWalker,
        _ctx: &mut CommandContext,
    ) -> Result<()> {
        walker.advance::<u64>();
        walker.advance::<u64>();
        Ok(())
    }

    fn before_send_sync_request(
        var: &Self,
        walker: &mut DataWalker,
        _ctx: &mut CommandContext,
    ) -> Result<()> {
        walker.advance_set(var.as_secs());
        walker.advance_set(var.subsec_nanos() as u64);
        Ok(())
    }
}

impl server::RequestCommandParameter<'_, core::time::Duration> for core::time::Duration {
    fn after_request_read(ctx: &mut server::ServerContext<'_>) -> Result<core::time::Duration> {
        let seconds: u64 = ctx.raw_data_walker.advance_get();
        let nanos: u64 = ctx.raw_data_walker.advance_get();

        Ok(core::time::Duration::new(seconds, nanos as u32))
    }
}

impl<E: Copy + Clone, T: Copy + Clone> client::RequestCommandParameter
    for EnumAsPrimitiveType<E, T>
{
    fn before_request_write(
        _raw: &Self,
        walker: &mut crate::ipc::DataWalker,
        _ctx: &mut crate::ipc::CommandContext,
    ) -> crate::result::Result<()> {
        walker.advance::<Self>();
        Ok(())
    }

    fn before_send_sync_request(
        raw: &Self,
        walker: &mut crate::ipc::DataWalker,
        _ctx: &mut crate::ipc::CommandContext,
    ) -> crate::result::Result<()> {
        walker.advance_set(*raw);
        Ok(())
    }
}

impl<E: Copy + Clone, T: Copy + Clone> client::ResponseCommandParameter<EnumAsPrimitiveType<E, T>>
    for EnumAsPrimitiveType<E, T>
{
    fn after_response_read(
        walker: &mut crate::ipc::DataWalker,
        _ctx: &mut crate::ipc::CommandContext,
    ) -> crate::result::Result<Self> {
        Ok(walker.advance_get())
    }
}

#[derive(Default)]
pub struct Session {
    pub object_info: ObjectInfo,
}

impl Session {
    pub const fn new() -> Self {
        Self {
            object_info: ObjectInfo::new(),
        }
    }

    pub const fn from(object_info: ObjectInfo) -> Self {
        Self { object_info }
    }

    pub const fn from_handle(handle: svc::Handle) -> Self {
        Self::from(ObjectInfo::from_handle(handle))
    }

    pub fn convert_to_domain(&mut self) -> Result<()> {
        self.object_info.domain_object_id = self.object_info.convert_current_object_to_domain()?;
        Ok(())
    }

    pub fn get_info(&mut self) -> &mut ObjectInfo {
        &mut self.object_info
    }

    pub fn set_info(&mut self, info: ObjectInfo) {
        self.object_info = info;
    }

    pub fn close(&mut self) {
        if self.object_info.is_valid() {
            if self.object_info.is_domain() {
                let mut ctx = CommandContext::new_client(self.object_info);
                cmif::client::write_request_command_on_msg_buffer(
                    &mut ctx,
                    None,
                    cmif::DomainCommandType::Close,
                );
                let _ = svc::send_sync_request(self.object_info.handle);
            } else if self.object_info.owns_handle {
                let mut ctx = CommandContext::new_client(self.object_info);

                match self.object_info.protocol {
                    CommandProtocol::Cmif => {
                        cmif::client::write_close_command_on_msg_buffer(&mut ctx)
                    }
                    CommandProtocol::Tipc => {
                        tipc::client::write_close_command_on_msg_buffer(&mut ctx)
                    }
                };

                let _ = svc::send_sync_request(self.object_info.handle);
            }
            if self.object_info.owns_handle {
                let _ = svc::close_handle(self.object_info.handle);
            }
            self.object_info = ObjectInfo::new();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

pub mod sm;

pub mod psm;

pub mod audio;

pub mod applet;

pub mod lm;

pub mod fatal;

pub mod dispdrv;

pub mod fsp;

pub mod hid;

pub mod nv;

pub mod vi;

pub mod hipc;

pub mod psc;

pub mod pm;

pub mod nfp;

pub mod mii;

pub mod set;

pub mod spl;

pub mod usb;

pub mod ldr;

pub mod ncm;

pub mod lr;

pub mod bsd;
