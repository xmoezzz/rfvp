//! Contains the machinery for Horizon OS's IPC interface

use crate::result::*;
use crate::svc;
use crate::thread;
use arrayvec::ArrayVec;
use core::mem;
use core::ptr;

pub mod rc;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum CommandProtocol {
    #[default]
    Cmif,
    Tipc,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ObjectInfo {
    pub handle: svc::Handle,
    pub domain_object_id: cmif::DomainObjectId,
    pub owns_handle: bool,
    pub protocol: CommandProtocol,
}

impl ObjectInfo {
    pub const fn new() -> Self {
        Self {
            handle: 0,
            domain_object_id: 0,
            owns_handle: false,
            protocol: CommandProtocol::Cmif,
        }
    }

    pub const fn from_handle(handle: svc::Handle) -> Self {
        Self {
            handle,
            domain_object_id: 0,
            owns_handle: true,
            protocol: CommandProtocol::Cmif,
        }
    }

    pub const fn from_domain_object_id(
        parent_handle: svc::Handle,
        domain_object_id: cmif::DomainObjectId,
    ) -> Self {
        Self {
            handle: parent_handle,
            domain_object_id,
            owns_handle: false,
            protocol: CommandProtocol::Cmif,
        }
    }

    pub const fn is_valid(&self) -> bool {
        self.handle != 0
    }

    pub const fn is_domain(&self) -> bool {
        self.domain_object_id != 0
    }

    pub fn uses_cmif_protocol(&self) -> bool {
        self.protocol == CommandProtocol::Cmif
    }

    pub fn uses_tipc_protocol(&self) -> bool {
        self.protocol == CommandProtocol::Tipc
    }

    pub fn convert_current_object_to_domain(&mut self) -> Result<cmif::DomainObjectId> {
        if self.uses_tipc_protocol() {
            return super::rc::ResultNotSupported::make_err();
        }
        ipc_client_send_control_command!([*self; cmif::ControlRequestId::ConvertCurrentObjectToDomain] () => (domain_object_id: cmif::DomainObjectId))
    }

    pub fn query_pointer_buffer_size(&self) -> Result<u16> {
        if self.uses_tipc_protocol() {
            return super::rc::ResultNotSupported::make_err();
        }
        ipc_client_send_control_command!([*self; cmif::ControlRequestId::QueryPointerBufferSize] () => (pointer_buffer_size: u16))
    }

    pub fn clone_current_object(&self) -> Result<sf::MoveHandle> {
        if self.uses_tipc_protocol() {
            return super::rc::ResultNotSupported::make_err();
        }
        ipc_client_send_control_command!([*self; cmif::ControlRequestId::CloneCurrentObject] () => (cloned_handle: sf::MoveHandle))
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BufferFlags {
    Normal = 0,
    NonSecure = 1,
    Invalid = 2,
    NonDevice = 3,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct BufferDescriptor {
    pub size_low: u32,
    pub address_low: u32,
    pub bits: u32,
}

impl BufferDescriptor {
    pub const fn empty() -> Self {
        Self {
            size_low: 0,
            address_low: 0,
            bits: 0,
        }
    }

    pub fn new(buffer: *const u8, buffer_size: usize, flags: BufferFlags) -> Self {
        let buffer = buffer.expose_provenance();
        let address_low = buffer as u32;
        let address_mid = (buffer >> 32) as u32;
        let address_high = (buffer >> 36) as u32;
        let size_low = buffer_size as u32;
        let size_high = (buffer_size >> 32) as u32;

        let mut bits: u32 = 0;
        write_bits!(0, 1, bits, flags as u32);
        write_bits!(2, 23, bits, address_high);
        write_bits!(24, 27, bits, size_high);
        write_bits!(28, 31, bits, address_mid);

        Self {
            size_low,
            address_low,
            bits,
        }
    }

    pub const fn get_address(&self) -> *mut u8 {
        let address_high = read_bits!(2, 23, self.bits);
        let address_mid = read_bits!(28, 31, self.bits);
        (self.address_low as usize
            | ((address_mid as usize) << 32)
            | ((address_high as usize) << 36)) as *mut u8
    }

    pub const fn get_size(&self) -> usize {
        let size_high = read_bits!(24, 27, self.bits);
        self.size_low as usize | ((size_high as usize) << 32)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct SendStaticDescriptor {
    bits: u32,
    address_low: u32,
}

impl SendStaticDescriptor {
    pub const fn empty() -> Self {
        Self {
            bits: 0,
            address_low: 0,
        }
    }

    pub fn new(buffer: *const u8, buffer_size: usize, index: u32) -> Self {
        let buffer = buffer.expose_provenance();
        let address_low = buffer as u32;
        let address_mid = (buffer >> 32) as u32;
        let address_high = (buffer >> 36) as u32;

        let mut bits: u32 = 0;
        write_bits!(0, 5, bits, index);
        write_bits!(6, 11, bits, address_high);
        write_bits!(12, 15, bits, address_mid);
        write_bits!(16, 31, bits, buffer_size as u32);

        Self { bits, address_low }
    }

    pub const fn get_address(&self) -> *mut u8 {
        let address_high = read_bits!(6, 11, self.bits);
        let address_mid = read_bits!(12, 15, self.bits);
        (self.address_low as usize
            | ((address_mid as usize) << 32)
            | ((address_high as usize) << 36)) as *mut u8
    }

    pub const fn get_size(&self) -> usize {
        read_bits!(16, 31, self.bits) as usize
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ReceiveStaticDescriptor {
    address_low: u32,
    bits: u32,
}

impl ReceiveStaticDescriptor {
    pub const fn empty() -> Self {
        Self {
            address_low: 0,
            bits: 0,
        }
    }

    pub fn new(buffer: *const u8, buffer_size: usize) -> Self {
        let buffer = buffer.expose_provenance();
        let address_low = buffer as u32;
        let address_high = (buffer >> 32) as u32;

        let mut bits: u32 = 0;
        write_bits!(0, 15, bits, address_high);
        write_bits!(16, 31, bits, buffer_size as u32);

        Self { address_low, bits }
    }

    pub const fn get_address(&self) -> *mut u8 {
        let address_high = read_bits!(0, 15, self.bits);
        (self.address_low as usize | ((address_high as usize) << 32)) as *mut u8
    }

    pub const fn get_size(&self) -> usize {
        read_bits!(16, 31, self.bits) as usize
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct CommandHeader {
    bits_1: u32,
    bits_2: u32,
}

impl CommandHeader {
    pub const fn empty() -> Self {
        Self {
            bits_1: 0,
            bits_2: 0,
        }
    }

    pub const fn encode_receive_static_type(receive_static_count: u32) -> u32 {
        let mut static_type: u32 = 0;
        if receive_static_count > 0 {
            static_type += 2;
            if receive_static_count != 0xFF {
                static_type += receive_static_count;
            }
        }
        static_type
    }

    pub const fn decode_receive_static_type(receive_static_type: u32) -> u32 {
        let mut count: u32 = 0;
        if receive_static_type > 0 {
            if receive_static_type == 2 {
                count = 0xFF;
            } else if receive_static_type > 2 {
                count = receive_static_type - 2;
            }
        }
        count
    }

    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        command_type: u32,
        send_static_count: u32,
        send_buffer_count: u32,
        receive_buffer_count: u32,
        exchange_buffer_count: u32,
        data_word_count: u32,
        receive_static_count: u32,
        has_special_header: bool,
    ) -> Self {
        let mut bits_1: u32 = 0;
        write_bits!(0, 15, bits_1, command_type);
        write_bits!(16, 19, bits_1, send_static_count);
        write_bits!(20, 23, bits_1, send_buffer_count);
        write_bits!(24, 27, bits_1, receive_buffer_count);
        write_bits!(28, 31, bits_1, exchange_buffer_count);

        let mut bits_2: u32 = 0;
        write_bits!(0, 9, bits_2, data_word_count);
        write_bits!(
            10,
            13,
            bits_2,
            Self::encode_receive_static_type(receive_static_count)
        );
        write_bits!(31, 31, bits_2, has_special_header as u32);

        Self { bits_1, bits_2 }
    }

    pub const fn get_command_type(&self) -> u32 {
        read_bits!(0, 15, self.bits_1)
    }

    pub const fn get_send_static_count(&self) -> u32 {
        read_bits!(16, 19, self.bits_1)
    }

    pub const fn get_send_buffer_count(&self) -> u32 {
        read_bits!(20, 23, self.bits_1)
    }

    pub const fn get_receive_buffer_count(&self) -> u32 {
        read_bits!(24, 27, self.bits_1)
    }

    pub const fn get_exchange_buffer_count(&self) -> u32 {
        read_bits!(28, 31, self.bits_1)
    }

    pub const fn get_data_word_count(&self) -> u32 {
        read_bits!(0, 9, self.bits_2)
    }

    pub const fn get_receive_static_count(&self) -> u32 {
        Self::decode_receive_static_type(read_bits!(10, 13, self.bits_2))
    }

    pub const fn get_has_special_header(&self) -> bool {
        read_bits!(31, 31, self.bits_2) != 0
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct CommandSpecialHeader {
    bits: u32,
}

impl CommandSpecialHeader {
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn new(
        send_process_id: bool,
        copy_handle_count: u32,
        move_handle_count: u32,
    ) -> Self {
        let mut bits: u32 = 0;
        write_bits!(0, 0, bits, send_process_id as u32);
        write_bits!(1, 4, bits, copy_handle_count);
        write_bits!(5, 8, bits, move_handle_count);

        Self { bits }
    }

    pub const fn get_send_process_id(&self) -> bool {
        read_bits!(0, 0, self.bits) != 0
    }

    pub const fn get_copy_handle_count(&self) -> u32 {
        read_bits!(1, 4, self.bits)
    }

    pub const fn get_move_handle_count(&self) -> u32 {
        read_bits!(5, 8, self.bits)
    }
}

pub const DATA_PADDING: u32 = 16;

const MAX_COUNT: usize = 8;

#[derive(Clone)]
pub struct DataWalker {
    ptr: *mut u8,
    cur_offset: isize,
}

impl DataWalker {
    pub fn empty() -> Self {
        Self {
            ptr: ptr::null_mut(),
            cur_offset: 0,
        }
    }

    pub fn new(ptr: *mut u8) -> Self {
        Self { ptr, cur_offset: 0 }
    }

    pub fn advance<T>(&mut self) {
        let align_of_type = core::mem::align_of::<T>() as isize;
        self.cur_offset += align_of_type - 1;
        self.cur_offset -= self.cur_offset % align_of_type;
        self.cur_offset += core::mem::size_of::<T>() as isize;
    }

    pub fn advance_get<T>(&mut self) -> T {
        unsafe {
            let align_of_type = core::mem::align_of::<T>() as isize;
            self.cur_offset += align_of_type - 1;
            self.cur_offset -= self.cur_offset % align_of_type;
            let offset = self.cur_offset;
            self.cur_offset += core::mem::size_of::<T>() as isize;

            let data_ref = self.ptr.offset(offset) as *const T;

            // even though we have aligned the offsets of the output data, we unfortunately don't know that the raw
            // data pointer will be aligned for out type, so we need to do an unaligned read (in libnx they just memcpy into the output object)
            data_ref.read_unaligned()
        }
    }

    pub fn advance_set<T>(&mut self, t: T) {
        unsafe {
            let align_of_type = core::mem::align_of::<T>() as isize;
            self.cur_offset += align_of_type - 1;
            self.cur_offset -= self.cur_offset % align_of_type;
            let offset = self.cur_offset;
            self.cur_offset += core::mem::size_of::<T>() as isize;

            let data_ref = self.ptr.offset(offset) as *mut T;

            // As above, we need an unaligned read just incase self.ptr doesn't have sufficiently large alignment
            data_ref.write_unaligned(t);
        }
    }

    pub fn reset(&mut self) {
        self.cur_offset = 0;
    }

    pub fn reset_with(&mut self, ptr: *mut u8) {
        self.reset();
        self.ptr = ptr;
    }

    pub fn get_offset(&self) -> isize {
        self.cur_offset
    }
}

#[inline(always)]
pub fn get_msg_buffer() -> *mut u8 {
    unsafe { (*thread::get_thread_local_region()).msg_buffer.as_mut_ptr() }
}

#[inline(always)]
/// Reads to an IPC array from a provided buffer
///
/// # Arguments
///
/// * `buffer`: In data buffer
/// * `count`:In data size in T-count, not bytes
/// * `array`: The ipc array to read the data from
///
/// # Safety
///
/// The caller is responsible for providing a pointer valid to read `count * size_of::<T>()` bytes
pub unsafe fn read_array_from_buffer<T: Copy, const LEN: usize>(
    buffer: *mut u8,
    count: u32,
    array: &mut ArrayVec<T, LEN>,
) -> *mut u8 {
    //debug_assert!(count <= MAX_COUNT, "Taking too may items from a data buffer");
    debug_assert!(
        is_aligned!(buffer as usize, align_of::<T>()),
        "Data buffer is not properly aligned"
    );

    array.clear();

    unsafe {
        let tmp_buffer = buffer.cast();
        let _ =
            array.try_extend_from_slice(core::slice::from_raw_parts(tmp_buffer, count as usize));
        tmp_buffer.add(count as usize).cast()
    }
}

/// Reads an IPC array into a provided buffer
///
/// # Arguments
///
/// * `buffer`: Out data buffer
/// * `count`: Out data size in T-count, not bytes
/// * `array`: The ipc array to read the data from
///
/// # Safety
///
/// The caller is responsible for providing a pointer valid to write `count * size_of::<T>()` bytes
#[inline(always)]
pub unsafe fn write_array_to_buffer<T: Copy, const LEN: usize>(
    buffer: *mut u8,
    count: u32,
    array: &ArrayVec<T, LEN>,
) -> *mut u8 {
    //debug_assert!(count <= MAX_COUNT, "Taking too may items from a data buffer");
    debug_assert!(
        is_aligned!(buffer as usize, align_of::<T>()),
        "Data buffer is not properly aligned"
    );

    debug_assert!(count as usize <= LEN, "Writing too may items to array");

    unsafe {
        let tmp_buffer = buffer as *mut T;
        core::ptr::copy(array.as_ptr(), tmp_buffer, count as usize);
        tmp_buffer.add(count as usize) as *mut u8
    }
}

#[inline(always)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn get_aligned_data_offset(data_words_offset: *mut u8, base_offset: *mut u8) -> *mut u8 {
    let align = DATA_PADDING as usize - 1;
    let data_offset = (data_words_offset.addr() - base_offset.addr() + align) & !align;
    unsafe { base_offset.add(data_offset) }
}

pub struct CommandContent {
    pub send_process_id: bool,
    pub process_id: u64,
    pub data_size: u32,
    pub data_offset: *mut u8,
    pub data_words_offset: *mut u8,
    pub objects_offset: *mut u8,
    copy_handles: ArrayVec<svc::Handle, MAX_COUNT>,
    move_handles: ArrayVec<svc::Handle, MAX_COUNT>,
    objects: ArrayVec<cmif::DomainObjectId, MAX_COUNT>,
    out_pointer_sizes: ArrayVec<u16, MAX_COUNT>,
}

impl CommandContent {
    pub fn empty() -> Self {
        Self {
            send_process_id: false,
            process_id: 0,
            data_size: 0,
            data_offset: ptr::null_mut(),
            data_words_offset: ptr::null_mut(),
            objects_offset: ptr::null_mut(),
            copy_handles: ArrayVec::new(),
            move_handles: ArrayVec::new(),
            objects: ArrayVec::new(),
            out_pointer_sizes: ArrayVec::new(),
        }
    }

    fn add_copy_handle(&mut self, handle: svc::Handle) -> Result<()> {
        match self.copy_handles.try_push(handle) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultCopyHandlesFull::make_err(),
        }
    }

    fn add_move_handle(&mut self, handle: svc::Handle) -> Result<()> {
        match self.move_handles.try_push(handle) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultMoveHandlesFull::make_err(),
        }
    }

    pub fn add_handle<const MOVE: bool>(&mut self, handle: sf::Handle<MOVE>) -> Result<()> {
        match MOVE {
            false => self.add_copy_handle(handle.handle),
            true => self.add_move_handle(handle.handle),
        }
    }

    pub fn add_domain_object(&mut self, domain_object_id: cmif::DomainObjectId) -> Result<()> {
        match self.objects.try_push(domain_object_id) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultDomainObjectsFull::make_err(),
        }
    }

    pub fn add_object(&mut self, object_info: ObjectInfo) -> Result<()> {
        if object_info.is_domain() {
            self.add_domain_object(object_info.domain_object_id)
        } else {
            rc::ResultInvalidDomainObject::make_err()
        }
    }

    fn add_out_pointer_size(&mut self, pointer_size: u16) -> Result<()> {
        match self.out_pointer_sizes.try_push(pointer_size) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultPointerSizesFull::make_err(),
        }
    }

    // rfvp patch: accept a handle of the other kind when the expected one is missing. Ryujinx
    // answers some commands (IHOSBinderDriver::GetNativeHandle) with a move handle where the
    // console sends a copy handle, and the strict check made surface creation fail.
    pub fn pop_copy_handle(&mut self) -> Result<svc::Handle> {
        match self.copy_handles.pop_at(0).or_else(|| self.move_handles.pop_at(0)) {
            Some(handle) => Ok(handle),
            None => cmif::rc::ResultInvalidOutObjectCount::make_err(),
        }
    }

    pub fn pop_move_handle(&mut self) -> Result<svc::Handle> {
        match self.move_handles.pop_at(0).or_else(|| self.copy_handles.pop_at(0)) {
            Some(handle) => Ok(handle),
            None => cmif::rc::ResultInvalidOutObjectCount::make_err(),
        }
    }

    pub fn pop_handle<const MOVE: bool>(&mut self) -> Result<sf::Handle<MOVE>> {
        let handle = match MOVE {
            false => sf::Handle::from(self.pop_copy_handle()?),
            true => sf::Handle::from(self.pop_move_handle()?),
        };
        Ok(handle)
    }

    fn push_copy_handle(&mut self, handle: svc::Handle) -> Result<()> {
        match self.copy_handles.try_push(handle) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultCopyHandlesFull::make_err(),
        }
    }

    fn push_move_handle(&mut self, handle: svc::Handle) -> Result<()> {
        match self.move_handles.try_push(handle) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultMoveHandlesFull::make_err(),
        }
    }

    pub fn push_handle<const MOVE: bool>(&mut self, handle: sf::Handle<MOVE>) -> Result<()> {
        match MOVE {
            false => self.push_copy_handle(handle.handle),
            true => self.push_move_handle(handle.handle),
        }
    }

    pub fn pop_domain_object(&mut self) -> Result<cmif::DomainObjectId> {
        match self.objects.pop_at(0) {
            Some(handle) => Ok(handle),
            None => cmif::rc::ResultInvalidOutObjectCount::make_err(),
        }
    }

    pub fn push_domain_object(&mut self, domain_object_id: cmif::DomainObjectId) -> Result<()> {
        match self.objects.try_push(domain_object_id) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultDomainObjectsFull::make_err(),
        }
    }
}

pub struct CommandContext {
    pub object_info: ObjectInfo,
    pub in_params: CommandContent,
    pub out_params: CommandContent,
    send_statics: ArrayVec<SendStaticDescriptor, MAX_COUNT>,
    receive_statics: ArrayVec<ReceiveStaticDescriptor, MAX_COUNT>,
    send_buffers: ArrayVec<BufferDescriptor, MAX_COUNT>,
    receive_buffers: ArrayVec<BufferDescriptor, MAX_COUNT>,
    exchange_buffers: ArrayVec<BufferDescriptor, MAX_COUNT>,
    pointer_buffer: *mut u8,
    in_pointer_buffer_offset: usize,
    out_pointer_buffer_offset: usize,
    pointer_size_walker: DataWalker,
    pointer_size_walker_initialized: bool,
}

impl CommandContext {
    pub fn empty() -> Self {
        Self {
            object_info: ObjectInfo::new(),
            in_params: CommandContent::empty(),
            out_params: CommandContent::empty(),
            send_statics: ArrayVec::new(),
            receive_statics: ArrayVec::new(),
            send_buffers: ArrayVec::new(),
            receive_buffers: ArrayVec::new(),
            exchange_buffers: ArrayVec::new(),
            pointer_buffer: core::ptr::null_mut(),
            in_pointer_buffer_offset: 0,
            out_pointer_buffer_offset: 0,
            pointer_size_walker: DataWalker::empty(),
            pointer_size_walker_initialized: false,
        }
    }

    pub fn new_client(object_info: ObjectInfo) -> Self {
        let mut ctx = Self::empty();
        ctx.object_info = object_info;
        ctx
    }

    fn ensure_pointer_size_walker(&mut self, raw_data_walker: &mut DataWalker) {
        if !self.pointer_size_walker_initialized {
            if self.object_info.uses_cmif_protocol() {
                let mut data_size = raw_data_walker.get_offset()
                    + DATA_PADDING as isize
                    + mem::size_of::<cmif::DataHeader>() as isize;
                if self.object_info.is_domain() {
                    data_size += (mem::size_of::<cmif::DomainInDataHeader>()
                        + mem::size_of::<cmif::DomainObjectId>() * self.in_params.objects.len())
                        as isize;
                }
                data_size = (data_size + 1) & !1;
                let out_pointer_sizes_offset =
                    unsafe { self.in_params.data_words_offset.offset(data_size) };
                self.pointer_size_walker = DataWalker::new(out_pointer_sizes_offset);
            }
            self.pointer_size_walker_initialized = true;
        }
    }

    pub fn new_server(object_info: ObjectInfo, pointer_buffer: *mut u8) -> Self {
        let mut ctx = Self::empty();
        ctx.object_info = object_info;
        ctx.pointer_buffer = pointer_buffer;
        ctx
    }

    fn add_send_static(&mut self, send_static: SendStaticDescriptor) -> Result<()> {
        match self.send_statics.try_push(send_static) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultSendStaticsFull::make_err(),
        }
    }

    fn add_receive_static(&mut self, receive_static: ReceiveStaticDescriptor) -> Result<()> {
        match self.receive_statics.try_push(receive_static) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultReceiveStaticsFull::make_err(),
        }
    }

    fn add_send_buffer(&mut self, send_buffer: BufferDescriptor) -> Result<()> {
        match self.send_buffers.try_push(send_buffer) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultSendBuffersFull::make_err(),
        }
    }

    fn add_receive_buffer(&mut self, receive_buffer: BufferDescriptor) -> Result<()> {
        match self.receive_buffers.try_push(receive_buffer) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultReceiveBuffersFull::make_err(),
        }
    }

    fn add_exchange_buffer(&mut self, exchange_buffer: BufferDescriptor) -> Result<()> {
        match self.exchange_buffers.try_push(exchange_buffer) {
            Ok(()) => Ok(()),
            Err(_) => rc::ResultExchangeBuffersFull::make_err(),
        }
    }

    pub fn add_buffer<
        const IN: bool,
        const OUT: bool,
        const MAP_ALIAS: bool,
        const POINTER: bool,
        const FIXED_SIZE: bool,
        const AUTO_SELECT: bool,
        const ALLOW_NON_SECURE: bool,
        const ALLOW_NON_DEVICE: bool,
        T,
    >(
        &mut self,
        buffer: &sf::Buffer<
            '_,
            IN,
            OUT,
            MAP_ALIAS,
            POINTER,
            FIXED_SIZE,
            AUTO_SELECT,
            ALLOW_NON_SECURE,
            ALLOW_NON_DEVICE,
            T,
        >,
    ) -> Result<()> {
        let buf_addr = buffer.get_address();
        let buf_size = buffer.get_size();

        if AUTO_SELECT {
            if self.pointer_buffer.is_null() {
                self.pointer_buffer = self.object_info.query_pointer_buffer_size()? as *mut u8;
            }

            let pointer_buf_size = self.pointer_buffer as usize;
            let mut buffer_in_static = false;
            if pointer_buf_size > 0 {
                let left_size = pointer_buf_size - self.in_pointer_buffer_offset;
                buffer_in_static = buf_size <= left_size;
            }
            if buffer_in_static {
                self.in_pointer_buffer_offset += buf_size;
            }

            if IN {
                if buffer_in_static {
                    self.add_send_buffer(BufferDescriptor::new(
                        ptr::null(),
                        0,
                        BufferFlags::Normal,
                    ))?;
                    self.add_send_static(SendStaticDescriptor::new(
                        buf_addr,
                        buf_size,
                        self.send_statics.len() as u32,
                    ))?;
                } else {
                    self.add_send_buffer(BufferDescriptor::new(
                        buf_addr,
                        buf_size,
                        BufferFlags::Normal,
                    ))?;
                    self.add_send_static(SendStaticDescriptor::new(
                        ptr::null(),
                        0,
                        self.send_statics.len() as u32,
                    ))?;
                }
            }
            if OUT {
                if buffer_in_static {
                    self.add_receive_buffer(BufferDescriptor::new(
                        ptr::null(),
                        0,
                        BufferFlags::Normal,
                    ))?;
                    self.add_receive_static(ReceiveStaticDescriptor::new(buf_addr, buf_size))?;
                    self.in_params.add_out_pointer_size(buf_size as u16)?;
                } else {
                    self.add_receive_buffer(BufferDescriptor::new(
                        buf_addr,
                        buf_size,
                        BufferFlags::Normal,
                    ))?;
                    self.add_receive_static(ReceiveStaticDescriptor::new(ptr::null(), 0))?;
                    self.in_params.add_out_pointer_size(0)?;
                }
            }
        } else if POINTER {
            if IN {
                self.add_send_static(SendStaticDescriptor::new(
                    buf_addr,
                    buf_size,
                    self.send_statics.len() as u32,
                ))?;
            }
            if OUT {
                self.add_receive_static(ReceiveStaticDescriptor::new(buf_addr, buf_size))?;
                if !FIXED_SIZE {
                    self.in_params.add_out_pointer_size(buf_size as u16)?;
                }
            }
        } else if MAP_ALIAS {
            let mut flags = BufferFlags::Normal;
            if ALLOW_NON_SECURE {
                flags = BufferFlags::NonSecure;
            } else if ALLOW_NON_DEVICE {
                flags = BufferFlags::NonDevice;
            }
            let buf_desc = BufferDescriptor::new(buf_addr, buf_size, flags);
            match (IN, OUT) {
                (true, true) => self.add_exchange_buffer(buf_desc),
                (true, false) => self.add_send_buffer(buf_desc),
                (false, true) => self.add_receive_buffer(buf_desc),
                (false, false) => Ok(()),
            }?;
        } else {
            return rc::ResultInvalidBufferAttributes::make_err();
        }

        Ok(())
    }

    fn pop_send_static(&mut self) -> Result<SendStaticDescriptor> {
        match self.send_statics.pop_at(0) {
            Some(send_static) => Ok(send_static),
            None => rc::ResultInvalidSendStaticCount::make_err(),
        }
    }

    fn pop_receive_static(&mut self) -> Result<ReceiveStaticDescriptor> {
        match self.receive_statics.pop_at(0) {
            Some(receive_static) => Ok(receive_static),
            None => rc::ResultInvalidReceiveStaticCount::make_err(),
        }
    }

    fn pop_send_buffer(&mut self) -> Result<BufferDescriptor> {
        match self.send_buffers.pop_at(0) {
            Some(send_buffer) => Ok(send_buffer),
            None => rc::ResultInvalidSendBufferCount::make_err(),
        }
    }

    fn pop_receive_buffer(&mut self) -> Result<BufferDescriptor> {
        match self.receive_buffers.pop_at(0) {
            Some(receive_buffer) => Ok(receive_buffer),
            None => rc::ResultInvalidReceiveBufferCount::make_err(),
        }
    }

    fn pop_exchange_buffer(&mut self) -> Result<BufferDescriptor> {
        match self.exchange_buffers.pop_at(0) {
            Some(exchange_buffer) => Ok(exchange_buffer),
            None => rc::ResultInvalidExchangeBufferCount::make_err(),
        }
    }

    fn pop_buffer<
        const IN: bool,
        const OUT: bool,
        const MAP_ALIAS: bool,
        const POINTER: bool,
        const FIXED_SIZE: bool,
        const AUTO_SELECT: bool,
        const ALLOW_NON_SECURE: bool,
        const ALLOW_NON_DEVICE: bool,
        T,
    >(
        &mut self,
        raw_data_walker: &mut DataWalker,
    ) -> Result<(*mut u8, usize)> {
        if AUTO_SELECT {
            if IN {
                if let Ok(static_desc) = self.pop_send_static()
                    && let Ok(send_desc) = self.pop_send_buffer()
                {
                    if !static_desc.get_address().is_null() && (static_desc.get_size() > 0) {
                        return Ok((static_desc.get_address(), static_desc.get_size()));
                    }
                    if !send_desc.get_address().is_null() && (send_desc.get_size() > 0) {
                        return Ok((send_desc.get_address(), send_desc.get_size()));
                    }
                }
            } else if OUT
                && let Ok(static_desc) = self.pop_receive_static()
                && let Ok(recv_desc) = self.pop_receive_buffer()
            {
                if !static_desc.get_address().is_null() && (static_desc.get_size() > 0) {
                    return Ok((static_desc.get_address(), static_desc.get_size()));
                }
                if !recv_desc.get_address().is_null() && (recv_desc.get_size() > 0) {
                    return Ok((recv_desc.get_address(), recv_desc.get_size()));
                }
            }
        } else if POINTER {
            if IN && let Ok(static_desc) = self.pop_send_static() {
                return Ok((static_desc.get_address(), static_desc.get_size()));
            } else if OUT {
                let buf_size = match FIXED_SIZE {
                    true => core::mem::size_of::<T>(),
                    false => {
                        self.ensure_pointer_size_walker(raw_data_walker);
                        self.pointer_size_walker.advance_get::<u16>() as usize
                    }
                };

                let buf = unsafe { self.pointer_buffer.add(self.out_pointer_buffer_offset) };
                self.out_pointer_buffer_offset += buf_size;
                return Ok((buf, buf_size));
            }
        } else if MAP_ALIAS {
            match (IN, OUT) {
                (true, true) => {
                    if let Ok(exch_desc) = self.pop_exchange_buffer() {
                        return Ok((exch_desc.get_address(), exch_desc.get_size()));
                    }
                }
                (true, false) => {
                    if let Ok(send_desc) = self.pop_send_buffer() {
                        return Ok((send_desc.get_address(), send_desc.get_size()));
                    }
                }
                (false, true) => {
                    if let Ok(recv_desc) = self.pop_receive_buffer() {
                        return Ok((recv_desc.get_address(), recv_desc.get_size()));
                    }
                }
                (false, false) => {}
            }
        }

        rc::ResultInvalidBufferAttributes::make_err()
    }

    pub fn pop_object(&mut self) -> Result<ObjectInfo> {
        if self.object_info.is_domain() {
            let domain_object_id = self.out_params.pop_domain_object()?;
            Ok(ObjectInfo::from_domain_object_id(
                self.object_info.handle,
                domain_object_id,
            ))
        } else {
            let handle: sf::MoveHandle = self.out_params.pop_handle()?;
            Ok(ObjectInfo::from_handle(handle.handle))
        }
    }
}

pub mod client;

pub mod server;

pub mod cmif;

pub mod tipc;

pub mod sf;
