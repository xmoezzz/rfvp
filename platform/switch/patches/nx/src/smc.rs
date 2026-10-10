//! Secure monitor support and wrappers

use crate::svc;
use core::mem as cmem;

/// Represents the secure monitor function IDs
///
/// Note that only those supported by this libraries are present in the enum...
/// See [SwitchBrew](https://switchbrew.org/wiki/SMC) for all options.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum FunctionId {
    Invalid = 0,
    //SetConfig = 0xC3000401,
    //GetConfig = 0xC3000002,
    //GetResult = 0xC3000003,
    //GetResultData = 0xC3000404,
    //MudularExponentiate = 0xC3000E05,
    GenerateRandomBytes = 0xC3000006,
    //GenerateAesKek = 0xC3000007,
    //LoadAesKey = 0xC3000008,
    //ComputeAes = 0xC3000009,
    //GenerateSpecificAesKey = 0xC300000A,
    //ComputeCmac = 0xC300040B,
    //DecryptAndImportEsDeviceKey = 0xC300100C,
    //ReencryptDeviceUniqueData = 0xC300D60C,
    //DecryptDeviceUniqueData = 0xC300100D,
    //DecryptAndImportLotusKey = 0xC300100E,
    //ModularExponentiateByStorageKey = 0xC300060F,
    //PrepareEsDeviceUniqueKey = 0xC3000610,
    //LoadPreparedAesKey = 0xC3000011,
    //PrepareEsCommonKey = 0xC3000012,
}

/// Represents the raw argument layout used in secure monitor calls
pub type Arguments = [u64; 8];

/// Represents the secure monitor call input layout (special case of [`Arguments`] for input)
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct Input {
    /// The calling function ID
    pub fn_id: FunctionId,
    /// The function-specific arguments
    pub args: [u64; 7],
}
const_assert!(cmem::size_of::<Input>() == 0x40);

impl Input {
    /// Creates a new, empty call [`Input`] with a certain function ID
    ///
    /// # Arguments
    ///
    /// * `fn_id`: Function ID value
    #[inline]
    pub const fn new(fn_id: FunctionId) -> Self {
        Self {
            fn_id,
            args: [0; 7],
        }
    }

    /// Converts this [`Input`] to the more generic [`Arguments`] layout
    #[inline]
    pub fn to_args(self) -> Arguments {
        unsafe { cmem::transmute(self) }
    }
}

/// Represents the result values returned on secure monitor call responses
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u64)]
pub enum Result {
    #[default]
    Success = 0,
    NotImplemented = 1,
    InvalidArgument = 2,
    InProgress = 3,
    NoAsyncOperation = 4,
    InvalidAsyncOperation = 5,
    NotPermitted = 6,
}

/// Represents the secure monitor call output layout (special case of [`Arguments`] for output)
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct Output {
    /// The response result
    pub result: Result,
    /// The response parameters
    pub params: [u64; 7],
}
const_assert!(cmem::size_of::<Output>() == cmem::size_of::<Input>());

impl Output {
    /// Creates an [`Output`] layout from a more generic [`Arguments`] layout
    ///
    /// # Arguments
    ///
    /// * `args`: The layout to create from
    #[inline]
    pub fn from_args(args: Arguments) -> Self {
        unsafe { cmem::transmute(args) }
    }
}

/// Represents the maximum size of the random bytes one can get in the `generate_random_bytes` SMC
///
/// This value is equivalent to the size of [`Output::params`]
pub const GENERATE_RANDOM_BYTES_MAX_SIZE: usize = 0x38;

/// Secure monitor call which generates random bytes
///
/// Note that the process needs to be running in processor 3 in order to be able to execute secure monitor calls
///
/// # Arguments
///
/// * `out_bytes`: Array to fill with random bytes, whose size mustn't exceed [`GENERATE_RANDOM_BYTES_MAX_SIZE`]
pub fn generate_random_bytes(out_bytes: &mut [u8]) -> Result {
    let mut input = Input::new(FunctionId::GenerateRandomBytes);
    input.args[0] = out_bytes.len() as u64;

    let output = Output::from_args(svc::call_secure_monitor(input.to_args()));
    if output.result == Result::Success {
        unsafe {
            core::ptr::copy(
                output.params.as_ptr() as *const u8,
                out_bytes.as_mut_ptr(),
                out_bytes.len(),
            );
        }
    }
    output.result
}
