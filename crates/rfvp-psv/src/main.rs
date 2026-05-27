#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::mem::MaybeUninit;
use core::panic::PanicInfo;

use rfvp::{RfvpBootConfig, RfvpCoreConfig};
use rfvp_psv::{PsvApp, RawPsvHost};

extern "C" {
    fn rfvp_psv_make_raw_host(out_host: *mut RawPsvHost) -> i32;
    fn rfvp_psv_platform_poll(app: *mut PsvApp) -> i32;
    fn rfvp_psv_platform_should_exit() -> i32;
    fn rfvp_psv_vitasdk_init();
    fn rfvp_psv_vitasdk_fini();
}

#[no_mangle]
pub unsafe extern "C" fn main(_argc: i32, _argv: *mut *mut u8) -> i32 {
    unsafe { rfvp_psv_vitasdk_init() };

    let mut raw_host = MaybeUninit::<RawPsvHost>::uninit();
    let status = unsafe { rfvp_psv_make_raw_host(raw_host.as_mut_ptr()) };
    if status != 0 {
        unsafe { rfvp_psv_vitasdk_fini() };
        return status;
    }

    let raw_host = unsafe { raw_host.assume_init() };
    let mut app = PsvApp::new(raw_host, RfvpCoreConfig::default(), 256);

    if let Err(err) = app.boot(RfvpBootConfig::default()) {
        let _ = app.render_frame();
        unsafe { rfvp_psv_vitasdk_fini() };
        return err_to_status(err);
    }

    loop {
        let poll_status = unsafe { rfvp_psv_platform_poll(&mut app) };
        if poll_status != 0 {
            unsafe { rfvp_psv_vitasdk_fini() };
            return poll_status;
        }

        if let Err(err) = app.run_frame() {
            unsafe { rfvp_psv_vitasdk_fini() };
            return err_to_status(err);
        }

        if app.quit_requested() || unsafe { rfvp_psv_platform_should_exit() } != 0 {
            break;
        }
    }

    unsafe { rfvp_psv_vitasdk_fini() };
    0
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {}
}

#[alloc_error_handler]
fn alloc_error(_layout: core::alloc::Layout) -> ! {
    loop {}
}

fn err_to_status(err: rfvp::host_api::RfvpError) -> i32 {
    match err {
        rfvp::host_api::RfvpError::Io => rfvp_psv::PsvStatus::Io.as_i32(),
        rfvp::host_api::RfvpError::NotFound => rfvp_psv::PsvStatus::NotFound.as_i32(),
        rfvp::host_api::RfvpError::InvalidData => rfvp_psv::PsvStatus::InvalidData.as_i32(),
        rfvp::host_api::RfvpError::InvalidArgument => rfvp_psv::PsvStatus::InvalidArgument.as_i32(),
        rfvp::host_api::RfvpError::Unsupported => rfvp_psv::PsvStatus::Unsupported.as_i32(),
        rfvp::host_api::RfvpError::OutOfMemory => rfvp_psv::PsvStatus::OutOfMemory.as_i32(),
        rfvp::host_api::RfvpError::CapacityExceeded => {
            rfvp_psv::PsvStatus::CapacityExceeded.as_i32()
        }
        rfvp::host_api::RfvpError::EndOfFile => rfvp_psv::PsvStatus::EndOfFile.as_i32(),
        rfvp::host_api::RfvpError::Backend => rfvp_psv::PsvStatus::Backend.as_i32(),
    }
}
