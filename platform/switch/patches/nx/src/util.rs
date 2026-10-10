//! Common library utilities

use crate::diag::abort;
use crate::diag::log;
use crate::diag::log::Logger;
use crate::result::*;
use crate::thread;

use alloc::string::String;
use alloc::string::ToString;
use core::fmt;
use core::panic;
use core::ptr;
use core::str;

use nx_derive::{Request, Response};

pub mod rc;

#[doc(hidden)]
pub trait AsInner<Inner: ?Sized> {
    fn as_inner(&self) -> &Inner;
}

#[doc(hidden)]
#[allow(dead_code)] // not used on all platforms
pub trait AsInnerMut<Inner: ?Sized> {
    fn as_inner_mut(&mut self) -> &mut Inner;
}

#[doc(hidden)]
pub trait IntoInner<Inner> {
    fn into_inner(self) -> Inner;
}

/// Represents a 16-byte UUID
#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct Uuid {
    /// The UUID byte array
    pub uuid: [u8; 0x10],
}

#[cfg(feature = "rand")]
impl Uuid {
    pub fn random() -> Result<Self> {
        use crate::ipc::sf::Buffer;
        use crate::service::spl::{IRandomClient, RandomService};

        let mut uuid = [8; 16];
        crate::service::new_service_object::<RandomService>()?
            .generate_random_bytes(Buffer::from_mut_array(&mut uuid))?;
        Ok(Self { uuid })
    }

    pub fn from_rng(rng: &mut impl nx::rand::RngCore) -> Result<Self> {
        let mut uuid = [8; 16];
        rng.fill_bytes(&mut uuid);
        Ok(Self { uuid })
    }
}

/// Represents a pair of a pointer and a size
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct PointerAndSize {
    /// The pointer address
    pub address: *mut u8,
    /// The pointer size
    pub size: usize,
}

impl PointerAndSize {
    /// Creates an empty, thus invalid [`PointerAndSize`] (with a null pointer and size `0`)
    #[inline]
    pub const fn empty() -> Self {
        Self {
            address: ptr::null_mut(),
            size: 0,
        }
    }

    /// Creates a [`PointerAndSize`]
    ///
    /// # Arguments
    ///
    /// * `address`: The address
    /// * `size`: The size
    #[inline]
    pub const fn new(address: *mut u8, size: usize) -> Self {
        Self { address, size }
    }

    /// Checks whether the [`PointerAndSize`] is valid
    ///
    /// Essentially, this checks that the pointer isn't null and that the size is non-zero
    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.address.is_null() && (self.size != 0)
    }
}

/// Represents a pair of a pointer and a size
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct ConstPointerAndSize {
    /// The pointer address
    pub address: *const u8,
    /// The pointer size
    pub size: usize,
}

impl ConstPointerAndSize {
    /// Creates an empty, thus invalid [`ConstPointerAndSize`] (with a null pointer and size `0`)
    #[inline]
    pub const fn empty() -> Self {
        Self {
            address: ptr::null(),
            size: 0,
        }
    }

    /// Creates a [`ConstPointerAndSize`]
    ///
    /// # Arguments
    ///
    /// * `address`: The address
    /// * `size`: The size
    #[inline]
    pub const fn new(address: *const u8, size: usize) -> Self {
        Self { address, size }
    }

    /// Checks whether the [`PointerAndSize`] is valid
    ///
    /// Essentially, this checks that the pointer isn't null and that the size is non-zero
    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.address.is_null() && (self.size != 0)
    }
}

pub(crate) const fn const_usize_min(a: usize, b: usize) -> usize {
    // TODO: const min traits
    if a > b { b } else { a }
}
#[allow(dead_code)]
pub(crate) const fn const_usize_max(a: usize, b: usize) -> usize {
    // TODO: const min traits
    if a < b { b } else { a }
}

/// Represents a C-like string of a given size (mostly like a C `char[S]` array)
///
/// Note that `char` is 4-bytes in Rust for encoding reasons, thus we must stick to `u8` arrays
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ArrayString<const S: usize> {
    /// The actual array (like `char[S]` in C)
    c_str: [u8; S],
}

impl<const S: usize> crate::ipc::server::RequestCommandParameter<'_, ArrayString<S>>
    for ArrayString<S>
{
    fn after_request_read(ctx: &mut crate::ipc::server::ServerContext) -> Result<Self> {
        Ok(ctx.raw_data_walker.advance_get())
    }
}

impl<const S: usize> crate::ipc::server::ResponseCommandParameter for ArrayString<S> {
    type CarryState = ();
    fn before_response_write(
        _raw: &Self,
        ctx: &mut crate::ipc::server::ServerContext,
    ) -> Result<()> {
        ctx.raw_data_walker.advance::<Self>();
        Ok(())
    }

    fn after_response_write(
        raw: Self,
        _carry_state: (),
        ctx: &mut crate::ipc::server::ServerContext,
    ) -> Result<()> {
        ctx.raw_data_walker.advance_set(raw);
        Ok(())
    }
}

impl<const S: usize> crate::ipc::client::RequestCommandParameter for ArrayString<S> {
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

impl<const S: usize> crate::ipc::client::ResponseCommandParameter<ArrayString<S>>
    for ArrayString<S>
{
    fn after_response_read(
        walker: &mut crate::ipc::DataWalker,
        _ctx: &mut crate::ipc::CommandContext,
    ) -> crate::result::Result<Self> {
        Ok(walker.advance_get())
    }
}

impl<const S: usize> fmt::Debug for ArrayString<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str_data = self.get_str().unwrap_or("<invalid-str>");
        write!(f, "{str_data}")
    }
}

impl<const S: usize> PartialEq for ArrayString<S> {
    fn eq(&self, other: &Self) -> bool {
        if let Ok(self_str) = self.get_str()
            && let Ok(other_str) = other.get_str()
        {
            return self_str == other_str;
        }
        false
    }
}

impl<const S: usize> Eq for ArrayString<S> {}

impl<const S: usize> Default for ArrayString<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const S: usize> ArrayString<S> {
    /// Creates an empty [`ArrayString`]
    pub const fn new() -> Self {
        Self { c_str: [0; S] }
    }

    /// Creates a [`ArrayString`] from a given byte array
    ///
    /// # Arguments
    ///
    /// * `raw_bytes`: Byte array to use
    pub const fn from_raw(raw_bytes: [u8; S]) -> Self {
        Self { c_str: raw_bytes }
    }

    /// Creates a [`ArrayString`] from a given `&str`
    ///
    /// This creates an empty [`ArrayString`] and initializes it with the provided string.
    /// This will copy at max `S - 1` bytes/chars in order to ensure that the string is NUL-terminated.
    /// This will truncate the string at the first null, so we can unconditionally return and keep it const.
    ///
    /// # Arguments
    ///
    /// * `string`: The `&str` to use
    pub const fn from_str_truncate_null(string: &str) -> Self {
        let mut out = Self::new();
        let string = string.as_bytes();
        let len = const_usize_min(S - 1, string.len());
        let mut offset = 0;
        // truncate at nuls since we're writing a cstr
        while offset < len && string[offset] != 0 {
            out.c_str[offset] = string[offset];
            offset += 1;
        }

        out
    }

    /// Creates a [`ArrayString`] from a given `&str`
    ///
    /// This creates an empty [`ArrayString`] and calls [`ArrayString::set_str`] on it
    ///
    /// # Arguments
    ///
    /// * `string`: The `&str` to use
    #[allow(clippy::should_implement_trait)] // We don't implement the trait as we do the conversion infallibly
    pub fn from_str(string: &str) -> Self {
        let mut cstr = Self::new();
        let _ = cstr.set_str(string);
        cstr
    }

    /// Creates a [`ArrayString`] from a given `String`
    ///
    /// This creates an empty [`ArrayString`] and calls [`ArrayString::set_string`] on it
    ///
    /// # Arguments
    ///
    /// * `string`: The `String` to use
    pub fn from_string(string: &String) -> Self {
        let mut cstr = Self::new();
        let _ = cstr.set_string(string);
        cstr
    }

    /// Returns the length of the [`ArrayString`]
    ///
    /// This is similar to C's `strlen()` function, thus taking into account the string's NUL-termination
    pub fn len(&self) -> usize {
        self.c_str.iter().position(|byte| *byte == 0).expect("We should always have at least one null as we always make sure to keep the last index null")
    }

    /// Returns whether this [`ArrayString`] is empty
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sets a `&str` as the contents of this [`ArrayString`]
    ///
    /// This will copy at max `S - 1` bytes/chars in order to ensure that the string is NUL-terminated
    /// Returns and error when the string has internal nulls. Truncates the written strings over `S-1` bytes in length.
    ///
    /// # Arguments
    ///
    /// * `string`: The `&str` to set
    pub fn set_str(&mut self, string: &str) -> Result<()> {
        // we're writing a c-string, so we can't have internal nuls
        result_return_if!(string.find('\0').is_some(), rc::ResultInvalidUtf8Conversion);

        self.c_str = [0; S];
        let string = string.as_bytes();
        let len = const_usize_min(S - 1, string.len());
        let mut offset = 0;
        while offset < len {
            self.c_str[offset] = string[offset];
            offset += 1;
        }

        Ok(())
    }

    /// Sets a string as the contents of this [`ArrayString`]
    ///
    /// This will copy at max `S - 1` bytes/chars in order to ensure that the string is NUL-terminated
    ///
    /// # Arguments
    ///
    /// * `string`: The content to set
    #[inline(always)]
    #[allow(clippy::ptr_arg)]
    pub fn set_string(&mut self, string: &String) -> Result<()> {
        self.set_str(string.as_str())
    }

    /// Gets a `&str` corresponding to this [`ArrayString`]
    pub fn get_str(&self) -> Result<&str> {
        core::ffi::CStr::from_bytes_until_nul(&self.c_str)
            .expect("We should never error as we always keep a null at the last index")
            .to_str()
            .map_err(|_| rc::ResultInvalidUtf8Conversion::make())
    }

    /// Gets a `String` corresponding to this [`ArrayString`]
    pub fn get_string(&self) -> Result<String> {
        self.get_str().map(Into::into)
    }

    /// Borrows a view into the whole array
    pub fn as_buffer(&self) -> &[u8; S] {
        &self.c_str
    }

    /// Borrows only the initialized bytes (including the null terminator)
    pub fn as_bytes(&self) -> &[u8] {
        &self.c_str[..(self.len() + 1)]
    }
}

impl<S: AsRef<str>, const LEN: usize> From<S> for ArrayString<LEN> {
    fn from(value: S) -> Self {
        let reffed_val: &str = value.as_ref();
        Self::from_str(reffed_val)
    }
}

/// Represents a C-like 16-bit string of a given size (mostly like a C `char16_t[S]` array)
///
/// Note that `char` is 4-bytes in Rust for encoding reasons, thus we must stick to `u16` arrays
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ArrayWideString<const S: usize> {
    /// The actual array (like `char16_t[S]` in C)
    c_wstr: [u16; S],
}

impl<const S: usize> crate::ipc::server::RequestCommandParameter<'_, ArrayWideString<S>>
    for ArrayWideString<S>
{
    fn after_request_read(ctx: &mut crate::ipc::server::ServerContext) -> Result<Self> {
        Ok(ctx.raw_data_walker.advance_get())
    }
}

impl<const S: usize> crate::ipc::server::ResponseCommandParameter for ArrayWideString<S> {
    type CarryState = ();
    fn before_response_write(
        _raw: &Self,
        ctx: &mut crate::ipc::server::ServerContext,
    ) -> Result<()> {
        ctx.raw_data_walker.advance::<Self>();
        Ok(())
    }

    fn after_response_write(
        raw: Self,
        _carry_state: (),
        ctx: &mut crate::ipc::server::ServerContext,
    ) -> Result<()> {
        ctx.raw_data_walker.advance_set(raw);
        Ok(())
    }
}

impl<const S: usize> crate::ipc::client::RequestCommandParameter for ArrayWideString<S> {
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

impl<const S: usize> crate::ipc::client::ResponseCommandParameter<ArrayWideString<S>>
    for ArrayWideString<S>
{
    fn after_response_read(
        walker: &mut crate::ipc::DataWalker,
        _ctx: &mut crate::ipc::CommandContext,
    ) -> crate::result::Result<Self> {
        Ok(walker.advance_get())
    }
}

impl<const S: usize> fmt::Debug for ArrayWideString<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str_data = self.get_string().unwrap_or("<invalid-str>".to_string());
        write!(f, "{str_data}")
    }
}

impl<const S: usize> PartialEq for ArrayWideString<S> {
    fn eq(&self, other: &Self) -> bool {
        self.c_wstr.as_slice().eq(other.c_wstr.as_slice())
    }
}

impl<const S: usize> Eq for ArrayWideString<S> {}

impl<const S: usize> Default for ArrayWideString<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const S: usize> ArrayWideString<S> {
    /// Creates an empty [`ArrayWideString`]
    pub const fn new() -> Self {
        Self { c_wstr: [0; S] }
    }

    /// Creates a [`ArrayWideString`] from a given byte array
    ///
    /// # Arguments
    ///
    /// * `raw_bytes`: Byte array to use
    pub const fn from_raw(raw_bytes: [u16; S]) -> Self {
        Self { c_wstr: raw_bytes }
    }

    /// Creates a [`ArrayWideString`] from a given `String`
    ///
    /// This creates an empty [`ArrayWideString`] and calls [`ArrayWideString::set_string`] on it
    ///
    /// # Arguments
    ///
    /// * `string`: The `String` to use
    pub fn from_string(string: String) -> Self {
        let mut cstr = Self::new();
        cstr.set_string(string);
        cstr
    }

    /// Returns the length of the [`ArrayWideString`]
    ///
    /// This is similar to C's `strlen()` function, thus taking into account the string's NUL-termination
    pub fn len(&self) -> usize {
        self.c_wstr
            .iter()
            .position(|word| *word == 0)
            .expect("We will have at least one null as we always keep the last index null")
    }

    /// Returns if this [`ArrayWideString`] is empty
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sets a `&str` as the contents of this [`ArrayWideString`]
    ///
    /// This will copy at max `S - 1` bytes/chars in order to ensure that the string is NUL-terminated
    ///
    /// # Arguments
    ///
    /// * `string`: The `&str` to set
    pub fn set_str(&mut self, string: &str) {
        let mut c_str = &mut self.c_wstr[..S - 1];
        let mut char_buf = [0u16; 2];
        for char in string.chars() {
            let encoded_char = char.encode_utf16(&mut char_buf);

            // we can't write any u16s if there aren't enough for surrogate pairs
            // so we bail out early
            if encoded_char.len() > c_str.len() {
                break;
            }
            // a character will always be at least one u16
            c_str[0] = encoded_char[0].to_be();

            // check if character required 4-byte encoding
            if encoded_char.len() == 2 {
                c_str[1] = encoded_char[1].to_be();
            }

            // advance the window by the length of the written u16 buffer
            c_str = &mut c_str[encoded_char.len()..]
        }
    }

    /// Sets a `String` as the contents of this [`ArrayWideString`]
    ///
    /// This will copy at max `S - 1` bytes/chars in order to ensure that the string is NUL-terminated
    ///
    /// # Arguments
    ///
    /// * `string`: The `String` to set
    pub fn set_string(&mut self, string: impl AsRef<str>) {
        self.set_str(string.as_ref())
    }

    /// Gets a `String` corresponding to this [`ArrayWideString`]
    pub fn get_string(&self) -> Result<String> {
        // create a clone of the internal buffer
        let mut tmp = self.c_wstr;

        // convert the u16s from big-endian encoding
        let _: () = tmp[..self.len()]
            .iter_mut()
            .map(|place| *place = u16::from_be(*place))
            .collect();

        // we don't need to use the endian version, since we've already converted from be-encoding
        let pulled_string = String::from_utf16(&tmp[..self.len()])
            .map_err(|_| rc::ResultInvalidUtf16Conversion::make())?;

        Ok(pulled_string)
    }

    /// Borrows a view into the whole array
    pub fn as_buffer(&self) -> &[u16; S] {
        &self.c_wstr
    }

    /// Borrows only the initialized bytes (including the null terminator)
    pub fn as_u16s(&self) -> &[u16] {
        &self.c_wstr[..(self.len() + 1)]
    }
}

impl<const S: usize> core::str::FromStr for ArrayWideString<S> {
    type Err = ResultCode;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        let mut cstr = Self::new();
        cstr.set_str(s);
        Ok(cstr)
    }
}

/// Same as C's `strlen()`
///
/// # Arguments
///
/// * `str_ptr`: The `const char*`-like ptr to use
///
/// # Safety
///
/// This is a wrapper around `Cstr::from_ptr`[`core::ffi::CStr::from_ptr`]
/// so the same safety requirements apply.
#[inline(always)]
pub unsafe fn str_ptr_len(str_ptr: *const u8) -> usize {
    unsafe { core::ffi::CStr::from_ptr(str_ptr.cast()).to_bytes().len() }
}

/// Simplified panic handler using a provided [`Logger`] type, available as a helpful default panic handler
///
/// This handler does the following:
/// * Logs the panic information via [`diag_log!`] macro and the provided [`Logger`] type
/// * Aborts with [`ResultPanicked`][`super::rc::ResultPanicked`] and the specified desired [`AbortLevel`][`abort::AbortLevel`]
///
/// # Arguments
///
/// * `info`: `PanicInfo` object got from the actual panic handler
/// * `desired_level`: Desired [`AbortLevel`][`abort::AbortLevel`] to abort with
pub fn simple_panic_handler<L: Logger>(
    info: &panic::PanicInfo,
    desired_level: abort::AbortLevel,
) -> ! {
    let thread_name = match unsafe { thread::current().as_ref() }.map(|t| t.name.get_str()) {
        Some(Ok(name)) => name,
        _ => "<unknown>",
    };
    diag_log!(L { log::LogSeverity::Fatal, true } => "Panic! at thread '{}' -> {}\n", thread_name, info);

    abort::abort(desired_level, super::rc::ResultPanicked::make())
}
