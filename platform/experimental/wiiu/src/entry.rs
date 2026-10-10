use rfvp::RfvpCoreConfig;

use crate::app::WiiUApp;
use crate::status::{rfvp_error_to_status, WiiUStatus};

const WIIU_EVENT_CAPACITY: usize = 256;

unsafe extern "C" {
    fn rfvp_wiiu_platform_should_exit() -> i32;
    fn rfvp_wiiu_platform_sleep_frame();
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_wiiu_app_main() -> i32 {
    let mut app = match WiiUApp::new(RfvpCoreConfig::default(), WIIU_EVENT_CAPACITY) {
        Ok(app) => app,
        Err(err) => return rfvp_error_to_status(err),
    };

    loop {
        if let Err(err) = app.run_empty_frame() {
            return rfvp_error_to_status(err);
        }
        if app.quit_requested() || unsafe { rfvp_wiiu_platform_should_exit() != 0 } {
            return WiiUStatus::Ok.as_i32();
        }
        unsafe {
            rfvp_wiiu_platform_sleep_frame();
        }
    }
}
