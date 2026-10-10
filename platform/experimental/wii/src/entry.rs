use core::ffi::c_void;

use rfvp::{RfvpBootConfig, RfvpCoreConfig};

use crate::app::WiiApp;
use crate::raw::RawWiiHost;
use crate::status::{rfvp_error_to_status, WiiStatus};

const WII_TARGET_WIDTH: u32 = 640;
const WII_TARGET_HEIGHT: u32 = 480;
const WII_EVENT_CAPACITY: usize = 128;
const WII_MAX_PENDING_EVENTS: usize = 256;
const WII_HCB_PAGE_BUDGET: usize = 2 * 1024 * 1024;
const WII_MAX_MANIFEST_ENTRIES: usize = 128;

unsafe extern "C" {
    fn rfvp_wii_platform_poll(app: *mut c_void) -> i32;
    fn rfvp_wii_platform_should_exit() -> i32;
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_wii_app_main(raw_host: *const RawWiiHost) -> i32 {
    if raw_host.is_null() {
        return WiiStatus::InvalidArgument.as_i32();
    }

    let raw_host = unsafe { *raw_host };
    let config = RfvpCoreConfig {
        virtual_width: WII_TARGET_WIDTH,
        virtual_height: WII_TARGET_HEIGHT,
        max_pending_events: WII_MAX_PENDING_EVENTS,
    };
    let mut app = WiiApp::new(
        raw_host,
        config,
        WII_EVENT_CAPACITY,
        WII_TARGET_WIDTH,
        WII_TARGET_HEIGHT,
    );

    let boot_config = RfvpBootConfig {
        asset_root: ".",
        max_hcb_bytes: WII_HCB_PAGE_BUDGET,
        max_manifest_entries: WII_MAX_MANIFEST_ENTRIES,
        ..RfvpBootConfig::default()
    };

    if let Err(err) = app.boot_old_school(boot_config) {
        return rfvp_error_to_status(err);
    }

    loop {
        let poll_status =
            unsafe { rfvp_wii_platform_poll((&mut app as *mut WiiApp).cast::<c_void>()) };
        if poll_status != WiiStatus::Ok.as_i32() {
            return poll_status;
        }

        if let Err(err) = app.run_frame() {
            return rfvp_error_to_status(err);
        }

        if app.quit_requested() || unsafe { rfvp_wii_platform_should_exit() != 0 } {
            return WiiStatus::Ok.as_i32();
        }
    }
}
