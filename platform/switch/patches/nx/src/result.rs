//! Common result support

use core::fmt;
use core::result;

use nx_derive::{Request, Response};

const MODULE_BITS: u32 = 9;
const DESCRIPTION_BITS: u32 = 13;
const DEFAULT_VALUE: u32 = 0;
const SUCCESS_VALUE: u32 = DEFAULT_VALUE;

#[inline]
pub(crate) const fn pack_value(module: u32, description: u32) -> u32 {
    module | (description << MODULE_BITS)
}

#[inline]
pub(crate) const fn unpack_module(value: u32) -> u32 {
    value & !(!DEFAULT_VALUE << MODULE_BITS)
}

#[inline]
pub(crate) const fn unpack_description(value: u32) -> u32 {
    (value >> MODULE_BITS) & !(!DEFAULT_VALUE << DESCRIPTION_BITS)
}

/// Represents a (raw) result value used all over the OS
///
/// These are referred as `Result` on docs/official code, but we intentionally name it as [`ResultCode`] to distinguish it from the [`Result`] enum type
///
/// Results are often displayed/shown, for example, like `2168-0002`, which corresponds to `<2000 + module>-<description>`
///
/// [`Debug`][`fmt::Debug`] formatting formats the results as a hex-value (`0x4A8`), while [`Display`][`fmt::Display`] formatting formats the result in the format described above (`2168-0002`)
#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct ResultCode {
    value: u32,
}

impl ResultCode {
    /// Creates a [`ResultCode`] from a raw value
    ///
    /// # Arguments
    ///
    /// * `value`: The raw value
    #[inline]
    pub const fn new(value: u32) -> Self {
        Self { value }
    }

    /// Wrapper for creating a new [`Result::Err`] value with the following raw result
    ///
    /// # Arguments
    ///
    /// * `value`: The raw value, note that it mustn't be `0`/success (that would be undefined behavior)
    #[inline]
    pub const fn new_err<T>(value: u32) -> Result<T> {
        Err(Self::new(value))
    }

    /// Returns whether the [`ResultCode`] is successful
    ///
    /// A result value of `0` is a successful value, this essentially checks that
    #[inline]
    pub const fn is_success(&self) -> bool {
        self.value == SUCCESS_VALUE
    }

    /// Returns whether the [`ResultCode`] is not successful
    ///
    /// This is the exact opposite of [`is_success`][`ResultCode::is_success`]
    #[inline]
    pub const fn is_failure(&self) -> bool {
        !self.is_success()
    }

    /// Gets the raw value of the [`ResultCode`]
    #[inline]
    pub const fn get_value(&self) -> u32 {
        self.value
    }

    /// Gets the module of the [`ResultCode`]
    #[inline]
    pub const fn get_module(&self) -> u32 {
        unpack_module(self.value)
    }

    /// Gets the description of the [`ResultCode`]
    #[inline]
    pub const fn get_description(&self) -> u32 {
        unpack_description(self.value)
    }
}

impl fmt::Debug for ResultCode {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(fmt, "{:#X}", self.value)
    }
}

impl fmt::Display for ResultCode {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(
            fmt,
            "{:0>4}-{:0>4}",
            2000 + self.get_module(),
            self.get_description()
        )
    }
}

/// Represents a result holding a certain value or a  [`ResultCode`] as an indication of failure
pub type Result<T> = result::Result<T, ResultCode>;

/// Produces a `Result` whose value will depend on whether the supplied [`ResultCode`] was successful
///
/// # Arguments
///
/// * `rc`: The [`ResultCode`] value
/// * `value`: The value to pack if the [`ResultCode`] is successful
#[inline(always)]
pub fn pack<T>(rc: ResultCode, value: T) -> Result<T> {
    if rc.is_success() { Ok(value) } else { Err(rc) }
}

/// Produces the [`ResultCode`] corresponding to a packed result
///
/// # Arguments
///
/// * `rc`: The [`Result`] to unpack
#[inline(always)]
pub fn unpack<T>(rc: &Result<T>) -> ResultCode {
    match rc {
        Ok(_) => ResultSuccess::make(),
        Err(rc) => *rc,
    }
}

/// Represents a base trait for result value definitions to follow
pub trait ResultBase {
    /// Gets the result definition's module
    fn get_module() -> u32;

    /// Gets the result definition's description
    fn get_description() -> u32;

    /// Gets the result definition's raw value
    #[inline(always)]
    fn get_value() -> u32 {
        pack_value(Self::get_module(), Self::get_description())
    }

    /// Produces a [`ResultCode`] from this result definition
    #[inline(always)]
    fn make() -> ResultCode {
        ResultCode::new(Self::get_value())
    }

    /// Produces a [`Result::Err`] value from this result definition
    #[inline(always)]
    fn make_err<T>() -> Result<T> {
        ResultCode::new_err(Self::get_value())
    }

    /// Returns whether the given [`ResultCode`] matches this result definition
    ///
    /// # Arguments
    ///
    /// * `rc`: The [`ResultCode`] to check
    #[inline(always)]
    fn matches(rc: ResultCode) -> bool {
        rc.get_value() == Self::get_value()
    }
}

// TODO: document all results? are the names not explicit enough?

result_define! {
    Success: 0, 0
}

#[cfg(any(feature = "fs", feature = "socket"))]
impl embedded_io::Error for ResultCode {
    fn kind(&self) -> embedded_io::ErrorKind {
        use embedded_io::ErrorKind;
        match (self.get_module(), self.get_description()) {
            #[cfg(feature = "socket")]
            (crate::socket::rc::RESULT_MODULE, errno) => match errno {
                1004 => ErrorKind::Interrupted,
                1005 => ErrorKind::WriteZero,
                1011 => ErrorKind::TimedOut,
                1032 => ErrorKind::BrokenPipe,
                _ => ErrorKind::Other,
            },
            #[cfg(feature = "fs")]
            (crate::fs::rc::RESULT_SUBMODULE, errno) => match errno {
                4000..=4999 => ErrorKind::InvalidData,
                6003..=6199 => ErrorKind::InvalidInput,
                6602 | 6603 => ErrorKind::NotFound,
                6300..=6399 => ErrorKind::Unsupported,
                6400..=6449 => ErrorKind::PermissionDenied,
                _ => ErrorKind::Other,
            },
            _ => ErrorKind::Other,
        }
    }
}
