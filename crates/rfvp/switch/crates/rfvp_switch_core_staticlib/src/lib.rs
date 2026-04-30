#![allow(unexpected_cfgs)]

#[cfg(not(rfvp_switch))]
compile_error!("rfvp_switch_core_staticlib must be built with RUSTFLAGS=\"--cfg rfvp_switch\"");

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_core_staticlib_force_link() -> u32 {
    rfvp::switch_core::rfvp_switch_core_abi_version()
}
