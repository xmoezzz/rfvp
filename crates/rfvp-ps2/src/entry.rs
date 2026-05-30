use core::ffi::c_void;

use rfvp::{RfvpBootConfig, RfvpCoreConfig};

use crate::app::Ps2App;
use crate::raw::RawPs2Host;
use crate::status::{rfvp_error_to_status, Ps2Status};

const PS2_TARGET_WIDTH: u32 = 640;
const PS2_TARGET_HEIGHT: u32 = 448;
const PS2_EVENT_CAPACITY: usize = 128;
const PS2_MAX_PENDING_EVENTS: usize = 256;
const PS2_HCB_PAGE_BUDGET: usize = 2 * 1024 * 1024;
const PS2_MAX_MANIFEST_ENTRIES: usize = 128;

unsafe extern "C" {
    fn rfvp_ps2_platform_poll(app: *mut c_void) -> i32;
    fn rfvp_ps2_platform_should_exit() -> i32;
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_ps2_app_main(raw_host: *const RawPs2Host) -> i32 {
    if raw_host.is_null() {
        return Ps2Status::InvalidArgument.as_i32();
    }

    let raw_host = unsafe { *raw_host };
    let config = RfvpCoreConfig {
        virtual_width: PS2_TARGET_WIDTH,
        virtual_height: PS2_TARGET_HEIGHT,
        max_pending_events: PS2_MAX_PENDING_EVENTS,
    };
    let mut app = Ps2App::new(
        raw_host,
        config,
        PS2_EVENT_CAPACITY,
        PS2_TARGET_WIDTH,
        PS2_TARGET_HEIGHT,
    );

    let boot_config = RfvpBootConfig {
        asset_root: ".",
        max_hcb_bytes: PS2_HCB_PAGE_BUDGET,
        max_manifest_entries: PS2_MAX_MANIFEST_ENTRIES,
        ..RfvpBootConfig::default()
    };

    if let Err(err) = app.boot_old_school(boot_config) {
        return rfvp_error_to_status(err);
    }

    loop {
        let poll_status =
            unsafe { rfvp_ps2_platform_poll((&mut app as *mut Ps2App).cast::<c_void>()) };
        if poll_status != Ps2Status::Ok.as_i32() {
            return poll_status;
        }

        if let Err(err) = app.run_frame() {
            return rfvp_error_to_status(err);
        }

        if app.quit_requested() || unsafe { rfvp_ps2_platform_should_exit() != 0 } {
            return Ps2Status::Ok.as_i32();
        }
    }
}
