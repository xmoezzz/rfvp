//! Generic library result definitions
//!
//! All `rc` modules in this library contain result definitions (usually related to/for the parent module)
//!
//! All library results have module [`RESULT_MODULE`], and their descriptions are `<mod-specific submodule> + <res-value>`
//!
//! For example, [`ResultNotImplemented`] has module [`RESULT_MODULE`] and description [`RESULT_SUBMODULE`] + `1`
//!
//! List of existing submodules in the library:
//!
//! * `0`: library (misc)
//! * `100`: elf
//! * `200`: (unused)
//! * `300`: util
//! * `400`: diag
//! * `500`: gpu
//! * `600`: ipc
//! * `700`: fs
//! * `800`: input
//! * `900`: thread
//! * `1000`: mem
//! * `1100`: gpu/binder
//! * `1200`: gpu/parcel
//! * `1300`: ipc/server

pub const RESULT_MODULE: u32 = 430;
/// Result submodule for the base `rc` module.
pub const RESULT_SUBMODULE: u32 = 0;

result_define_subgroup!(RESULT_MODULE, RESULT_SUBMODULE => {
    NotImplemented: 1,
    NotSupported: 2,
    NotInitialized: 3,
    Panicked: 4
});

/*

- Submodule list for our own results:

0: library (misc)
100: dynamic
200: dynamic/elf
300: util
400: diag
500: gpu
600: ipc
700: fs
800: input
900: thread
1000: mem
1100: gpu/binder
1200: gpu/parcel
1300: ipc/server

*/
