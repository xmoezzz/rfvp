//! Parcel support and utils

use alloc::vec::Vec;

use crate::result::*;
use crate::service::dispdrv::BinderHandle;
use crate::util;
use core::ptr;

pub mod rc;

/// Represents a parcel header layout
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ParcelHeader {
    /// The payload size
    pub payload_size: u32,
    /// The payload offset
    pub payload_offset: u32,
    /// The object list size
    pub objects_size: u32,
    /// The object list offset
    pub objects_offset: u32,
}

impl ParcelHeader {
    /// Creates a new, empty [`ParcelHeader`]
    #[inline]
    pub const fn new() -> Self {
        Self {
            payload_size: 0,
            payload_offset: 0,
            objects_size: 0,
            objects_offset: 0,
        }
    }
}

const PAYLOAD_SIZE: usize = 0x200;

/// Represents a parcel payload layout
///
/// Note that a parcel payload length is variable, but we use a maximum size for this type
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ParcelPayload {
    /// The header
    pub header: ParcelHeader,
    /// The actual payload
    pub payload: [u8; PAYLOAD_SIZE],
}

impl ParcelPayload {
    /// Creates a new, empty [`ParcelPayload`]
    #[inline]
    pub const fn new() -> Self {
        Self {
            header: ParcelHeader::new(),
            payload: [0; PAYLOAD_SIZE],
        }
    }
}

impl Default for ParcelPayload {
    fn default() -> Self {
        Self::new()
    }
}

/// Carrier for the layer's parameters. Used to receive the layer's binder handle.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ParcelData {
    /// The parcel type, usually `0x2`
    pub parcel_type: u32,
    /// Unknown, maybe the process ID
    pub unk_maybe_pid: u32,
    /// Binder handle
    pub handle: BinderHandle,
    /// Unknown, usually zeros
    pub unk_zero: [u8; 0xC],
    /// NUL-terminated string containing `"dispdrv\0"`
    pub dispdrv_str: util::ArrayString<0x8>,
    /// Unknown, usually zeros
    pub unk_zero_2: [u8; 8],
}

/// Represents a wrapper for simple parcel reading/writing
pub struct Parcel {
    payload: ParcelPayload,
    read_offset: usize,
    write_offset: usize,
}

impl Parcel {
    /// Creates a new [`Parcel`]
    #[inline]
    pub const fn new() -> Self {
        Self {
            payload: ParcelPayload::new(),
            read_offset: 0,
            write_offset: 0,
        }
    }

    /// Reads raw, unaligned data
    ///
    /// # Arguments
    ///
    /// * `out_data`: Out data buffer
    /// * `data_size`: Out data size
    ///
    /// # Safety
    ///
    /// The caller is responsible for providing a pointer valid to write `out_size` bytes
    pub unsafe fn read_raw_unaligned(&mut self, out_data: *mut u8, data_size: usize) -> Result<()> {
        result_return_if!(
            (self.read_offset + data_size) > PAYLOAD_SIZE,
            rc::ResultNotEnoughReadSpace
        );

        unsafe {
            ptr::copy(
                self.payload.payload.as_mut_ptr().add(self.read_offset),
                out_data,
                data_size,
            );
        }
        self.read_offset += data_size;
        Ok(())
    }

    /// Reads raw (aligned) data
    ///
    /// This essentially aligns up the read size to a 4-byte align
    ///
    /// # Arguments
    ///
    /// * `out_data`: Out data buffer
    /// * `data_size`: Out data size
    ///
    /// # Safety
    ///
    /// The caller is responsible for providing a pointer valid to write `out_size` bytes
    #[inline]
    pub unsafe fn read_raw(&mut self, out_data: *mut u8, data_size: usize) -> Result<()> {
        unsafe { self.read_raw_unaligned(out_data, data_size) }?;
        self.read_offset += (align_up!(data_size, 4usize) - data_size);
        Ok(())
    }

    /// Writes raw, unaligned data
    ///
    /// # Arguments
    ///
    /// * `data`: In data buffer
    /// * `data_size`: In data size
    ///
    /// # Safety
    ///
    /// The caller is responsible for providing a pointer valid to read `out_size` bytes
    pub unsafe fn write_raw_unaligned(&mut self, data: *const u8, data_size: usize) -> Result<()> {
        result_return_if!(
            (self.write_offset + data_size) > PAYLOAD_SIZE,
            rc::ResultNotEnoughWriteSpace
        );

        unsafe {
            ptr::copy(
                data,
                self.payload.payload.as_mut_ptr().add(self.write_offset),
                data_size,
            );
        }
        self.write_offset += data_size;
        Ok(())
    }

    /// Reserves a certain (aligned) size at the payload, to be written later (returning the corresponding buffer)
    ///
    /// # Arguments
    ///
    /// * `data_size`: Out data size
    pub fn write_reserve_raw(&mut self, data_size: usize) -> Result<*mut u8> {
        let actual_size = align_up!(data_size, 4usize);
        result_return_if!(
            (self.write_offset + actual_size) > PAYLOAD_SIZE,
            rc::ResultNotEnoughWriteSpace
        );

        let buf = unsafe { self.payload.payload.as_mut_ptr().add(self.write_offset) };
        self.write_offset += actual_size;
        Ok(buf)
    }

    /// Writes raw (aligned) data
    ///
    /// This essentially aligns up the write size to a 4-byte alignment in the buffer
    ///
    /// # Arguments
    ///
    /// * `data`: In data buffer
    /// * `data_size`: In data size
    ///
    /// # Safety
    ///
    /// The caller must provide a pointer valid for reading `data_size` bytes
    #[inline]
    pub unsafe fn write_raw(&mut self, data: *const u8, data_size: usize) -> Result<()> {
        unsafe { self.write_raw_unaligned(data, data_size) }?;
        self.write_offset += (align_up!(data_size, 4usize) - data_size);
        Ok(())
    }

    /// Writes an unaligned value
    ///
    /// # Arguments
    ///
    /// * `t`: The value
    #[inline]
    pub fn write_unaligned<T>(&mut self, t: T) -> Result<()> {
        unsafe { self.write_raw_unaligned((&raw const t).cast(), size_of::<T>()) }
    }

    /// Writes a value (aligned)
    ///
    /// # Arguments
    ///
    /// * `t`: The value
    #[inline]
    pub fn write<T: Copy>(&mut self, t: T) -> Result<()> {
        unsafe { self.write_raw((&raw const t).cast(), size_of::<T>()) }
    }

    /// Reads an unaligned value
    pub fn read_unaligned<T>(&mut self) -> Result<T> {
        unsafe {
            let mut t: T = core::mem::zeroed();
            self.read_raw_unaligned((&raw mut t).cast(), size_of::<T>())?;
            Ok(t)
        }
    }

    /// Reads a value (aligned to a 4-byte word)
    pub fn read<T: Copy>(&mut self) -> Result<T> {
        unsafe {
            let mut t: T = core::mem::zeroed();
            self.read_raw((&raw mut t).cast(), size_of::<T>())?;
            Ok(t)
        }
    }

    /// Writes a string
    ///
    /// Note that strings are internally (de)serialized as NUL-terminated UTF-16
    ///
    /// # Arguments
    ///
    /// * `string`: The string to write
    pub fn write_str(&mut self, string: &str) -> Result<()> {
        let encoded: Vec<u16> = string.encode_utf16().collect();
        let len = encoded.len();
        self.write(len as u32)?;
        let str_write_buf = self.write_reserve_raw((len + 1) * 2)? as *mut u16;

        unsafe { core::ptr::copy(encoded.as_ptr(), str_write_buf, encoded.len()) };
        Ok(())
    }

    /// Writes an interface token
    ///
    /// # Arguments
    ///
    /// * `token`: The interface token name
    pub fn write_interface_token(&mut self, token: &str) -> Result<()> {
        let value: u32 = 0x100;
        self.write(value)?;
        self.write_str(token)
    }

    /// Reads raw sized data
    ///
    /// For sized data, the data is preceded by its size
    ///
    /// # Arguments
    ///
    /// * `out_data`: Out data buffer
    /// * `out_size`: The maximum size in bytes that can be read into `out_data`
    ///
    /// # Safety
    ///
    /// The caller is responsible for providing a pointer valid to read `out_size` bytes
    pub unsafe fn read_sized_raw(&mut self, out_data: *mut u8, out_size: usize) -> Result<usize> {
        let len = self.read::<i32>()? as usize;
        result_return_unless!(len <= out_size, rc::ResultReadSizeMismatch);
        let fd_count = self.read::<i32>()?;
        result_return_unless!(fd_count == 0, rc::ResultFdsNotSupported);

        unsafe {
            self.read_raw(out_data, len)?;
        }
        Ok(len)
    }

    /// Reads a value as sized data
    ///
    /// This verifies that the read data is at least big enough to contain the value type, returning [`ResultReadSizeMismatch`][`rc::ResultReadSizeMismatch`] otherwise
    pub fn read_sized<T: Default + Copy>(&mut self) -> Result<T> {
        let mut t: T = Default::default();
        let _len = unsafe { self.read_sized_raw((&raw mut t).cast(), size_of::<T>())? };
        Ok(t)
    }

    /// Writes raw sized data
    ///
    /// # Arguments
    ///
    /// * `data`: In data buffer
    /// * `data size`: In data size
    ///
    /// # Safety
    ///
    /// The caller must provide a pointer valid for reading `data_size` bytes
    pub unsafe fn write_sized_raw(&mut self, data: *const u8, data_size: usize) -> Result<()> {
        let len = data_size as i32;
        self.write(len)?;
        let fd_count: i32 = 0;
        self.write(fd_count)?;

        unsafe { self.write_raw(data, data_size)? };
        Ok(())
    }

    /// Writes a value as sized data
    ///
    /// # Arguments
    ///
    /// * `t`: The value to write
    #[inline]
    pub fn write_sized<T: Copy>(&mut self, t: T) -> Result<()> {
        unsafe { self.write_sized_raw((&raw const t).cast(), size_of::<T>()) }
    }

    /// Loads an external payload in this [`Parcel`]
    ///
    /// # Arguments
    ///
    /// * `payload`: The payload
    pub fn load_from(&mut self, payload: ParcelPayload) {
        self.payload = payload;
        self.read_offset = 0;
        self.write_offset = payload.header.payload_size as usize;
    }

    /// Finishes writing and produces the payload
    ///
    /// Essentially populates the payload header and returns the current payload, along with its size
    pub fn end_write(&mut self) -> Result<(ParcelPayload, usize)> {
        self.payload.header.payload_size = self.write_offset as u32;
        self.payload.header.payload_offset = size_of::<ParcelHeader>() as u32;
        let payload_len = self.payload.header.payload_offset + self.payload.header.payload_size;
        self.payload.header.objects_offset = payload_len;
        self.payload.header.objects_size = 0;
        Ok((self.payload, payload_len as usize))
    }
}

impl Default for Parcel {
    fn default() -> Self {
        Self::new()
    }
}
