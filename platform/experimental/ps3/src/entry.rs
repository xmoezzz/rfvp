use rfvp::RfvpCoreConfig;

use crate::app::PS3App;
use crate::status::{rfvp_error_to_status, PS3Status};

const PS3_EVENT_CAPACITY: usize = 256;

unsafe extern "C" {
    fn rfvp_ps3_platform_should_exit() -> i32;
    fn rfvp_ps3_platform_sleep_frame();
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_ps3_app_main() -> i32 {
    let mut app = match PS3App::new(RfvpCoreConfig::default(), PS3_EVENT_CAPACITY) {
        Ok(app) => app,
        Err(err) => return rfvp_error_to_status(err),
    };

    loop {
        if let Err(err) = app.run_empty_frame() {
            return rfvp_error_to_status(err);
        }
        if app.quit_requested() || unsafe { rfvp_ps3_platform_should_exit() != 0 } {
            return PS3Status::Ok.as_i32();
        }
        unsafe {
            rfvp_ps3_platform_sleep_frame();
        }
    }
}
