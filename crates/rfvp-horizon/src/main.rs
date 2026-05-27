#![no_std]
#![no_main]

extern crate alloc;
extern crate nx;

use core::panic::PanicInfo;

use nx::diag::abort;
use nx::diag::log::lm::LmLogger;
use nx::result::Result;
use nx::svc;
use nx::util;
use nx::util::PointerAndSize;
use rfvp::RfvpCoreConfig;
use rfvp_horizon::{horizon_status_to_result_code, HorizonApp};

nx::rrt0_define_module_name!("rfvp-horizon");

#[no_mangle]
pub fn initialize_heap(hbl_heap: PointerAndSize) -> PointerAndSize {
    if hbl_heap.is_valid() {
        hbl_heap
    } else {
        nx::mem::alloc::configure_heap(hbl_heap)
    }
}

#[no_mangle]
pub fn main() -> Result<()> {
    let mut app = match HorizonApp::new(RfvpCoreConfig::default(), 256) {
        Ok(app) => app,
        Err(err) => return Err(horizon_status_to_result_code(err)),
    };

    loop {
        if let Err(err) = app.run_empty_frame() {
            return Err(horizon_status_to_result_code(err));
        }
        if app.quit_requested() {
            break;
        }
        svc::sleep_thread(16_666_667)?;
    }

    Ok(())
}

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    util::simple_panic_handler::<LmLogger>(info, abort::AbortLevel::FatalThrow())
}
