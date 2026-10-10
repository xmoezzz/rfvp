use core::ffi::c_void;

use rfvp::{RfvpBootConfig, RfvpCoreConfig};

use crate::app::ThreeDsApp;
use crate::raw::RawThreeDsHost;
use crate::status::{rfvp_error_to_status, ThreeDsStatus};

const THREE_DS_TARGET_WIDTH: u32 = 400;
const THREE_DS_TARGET_HEIGHT: u32 = 240;
const THREE_DS_EVENT_CAPACITY: usize = 128;
const THREE_DS_MAX_PENDING_EVENTS: usize = 256;
const THREE_DS_HCB_PAGE_BUDGET: usize = 2 * 1024 * 1024;
const THREE_DS_MAX_MANIFEST_ENTRIES: usize = 128;

unsafe extern "C" {
    fn rfvp_3ds_platform_poll(app: *mut c_void) -> i32;
    fn rfvp_3ds_platform_should_exit() -> i32;
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_3ds_app_main(raw_host: *const RawThreeDsHost) -> i32 {
    if raw_host.is_null() {
        return ThreeDsStatus::InvalidArgument.as_i32();
    }

    let raw_host = unsafe { *raw_host };
    let config = RfvpCoreConfig {
        virtual_width: THREE_DS_TARGET_WIDTH,
        virtual_height: THREE_DS_TARGET_HEIGHT,
        max_pending_events: THREE_DS_MAX_PENDING_EVENTS,
    };
    let mut app = ThreeDsApp::new(
        raw_host,
        config,
        THREE_DS_EVENT_CAPACITY,
        THREE_DS_TARGET_WIDTH,
        THREE_DS_TARGET_HEIGHT,
    );

    let boot_config = RfvpBootConfig {
        asset_root: ".",
        max_hcb_bytes: THREE_DS_HCB_PAGE_BUDGET,
        max_manifest_entries: THREE_DS_MAX_MANIFEST_ENTRIES,
        ..RfvpBootConfig::default()
    };

    if let Err(err) = app.boot_old_school(boot_config) {
        return rfvp_error_to_status(err);
    }

    loop {
        let poll_status =
            unsafe { rfvp_3ds_platform_poll((&mut app as *mut ThreeDsApp).cast::<c_void>()) };
        if poll_status != ThreeDsStatus::Ok.as_i32() {
            return poll_status;
        }

        if let Err(err) = app.run_frame() {
            return rfvp_error_to_status(err);
        }

        if app.quit_requested() || unsafe { rfvp_3ds_platform_should_exit() != 0 } {
            return ThreeDsStatus::Ok.as_i32();
        }
    }
}
