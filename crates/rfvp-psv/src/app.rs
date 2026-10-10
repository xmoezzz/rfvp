use alloc::boxed::Box;

use rfvp::host_api::{InputModifiers, KeyCode, RfvpEvent, RfvpResult};
use rfvp::{RfvpBootConfig, RfvpCore, RfvpCoreConfig, RfvpTickResult};

use crate::event::PsvEventQueue;
use crate::host::PsvHost;
use crate::raw::RawPsvHost;

pub struct PsvApp {
    core: RfvpCore,
    host: PsvHost,
    events: PsvEventQueue,
}

impl PsvApp {
    pub fn new(raw_host: RawPsvHost, config: RfvpCoreConfig, event_capacity: usize) -> Self {
        Self {
            core: RfvpCore::new(config),
            host: PsvHost::from_raw(raw_host),
            events: PsvEventQueue::new(event_capacity),
        }
    }

    pub fn core(&self) -> &RfvpCore {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut RfvpCore {
        &mut self.core
    }

    pub fn host(&mut self) -> &mut PsvHost {
        &mut self.host
    }

    pub fn push_event(&mut self, event: RfvpEvent) -> RfvpResult<()> {
        self.events.push(event)
    }

    pub fn boot(&mut self, config: RfvpBootConfig<'_>) -> RfvpResult<()> {
        self.core.boot(&mut self.host, config)
    }

    /// Pointer positions arrive in screen pixels; the game wants its own virtual coordinates
    /// (the screen shows the virtual frame scaled and letterboxed).
    fn to_virtual(&self, event: RfvpEvent) -> RfvpEvent {
        let map = |x: i32, y: i32| {
            let (vx, vy) = self.host.renderer_ref().screen_to_virtual(x as f32, y as f32);
            (vx as i32, vy as i32)
        };
        match event {
            RfvpEvent::PointerMove { x, y, in_screen } => {
                let (x, y) = map(x, y);
                RfvpEvent::PointerMove { x, y, in_screen }
            }
            RfvpEvent::PointerDown { button, x, y } => {
                let (x, y) = map(x, y);
                RfvpEvent::PointerDown { button, x, y }
            }
            RfvpEvent::PointerUp { button, x, y } => {
                let (x, y) = map(x, y);
                RfvpEvent::PointerUp { button, x, y }
            }
            RfvpEvent::TouchDown { id, x, y } => {
                let (x, y) = map(x, y);
                RfvpEvent::TouchDown { id, x, y }
            }
            RfvpEvent::TouchMove { id, x, y } => {
                let (x, y) = map(x, y);
                RfvpEvent::TouchMove { id, x, y }
            }
            RfvpEvent::TouchUp { id, x, y } => {
                let (x, y) = map(x, y);
                RfvpEvent::TouchUp { id, x, y }
            }
            other => other,
        }
    }

    /// Deliver input, advance the script and draw. The core draws and presents the frame itself.
    pub fn tick(&mut self) -> RfvpResult<RfvpTickResult> {
        for index in 0..self.events.as_slice().len() {
            let event = self.to_virtual(self.events.as_slice()[index]);
            self.core.push_event(event)?;
        }
        self.events.clear();
        self.core.tick(&mut self.host)
    }

    pub fn render_frame(&mut self) -> RfvpResult<()> {
        self.core.render_status_frame(&mut self.host)
    }

    pub fn render_empty_frame(&mut self) -> RfvpResult<()> {
        self.render_frame()
    }

    pub fn run_frame(&mut self) -> RfvpResult<RfvpTickResult> {
        self.tick()
    }

    pub fn run_empty_frame(&mut self) -> RfvpResult<RfvpTickResult> {
        self.run_frame()
    }

    pub fn quit_requested(&self) -> bool {
        self.core.quit_requested()
    }
}

impl Drop for PsvApp {
    fn drop(&mut self) {
        unsafe {
            rfvp_psv_c_audio_shutdown();
            rfvp_psv_c_renderer_shutdown();
        }
    }
}

extern "C" {
    fn rfvp_psv_c_audio_shutdown();
    fn rfvp_psv_c_renderer_shutdown();
}

#[no_mangle]
pub extern "C" fn rfvp_psv_app_create(
    raw_host: RawPsvHost,
    virtual_width: u32,
    virtual_height: u32,
    max_pending_events: usize,
    event_capacity: usize,
) -> *mut PsvApp {
    let config = RfvpCoreConfig {
        virtual_width,
        virtual_height,
        max_pending_events,
    };
    Box::into_raw(Box::new(PsvApp::new(raw_host, config, event_capacity)))
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_destroy(app: *mut PsvApp) {
    if !app.is_null() {
        unsafe {
            drop(Box::from_raw(app));
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_run_empty_frame(app: *mut PsvApp) -> i32 {
    if app.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    match app.run_empty_frame() {
        Ok(_) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => match err {
            rfvp::host_api::RfvpError::Io => crate::status::PsvStatus::Io.as_i32(),
            rfvp::host_api::RfvpError::NotFound => crate::status::PsvStatus::NotFound.as_i32(),
            rfvp::host_api::RfvpError::InvalidData => {
                crate::status::PsvStatus::InvalidData.as_i32()
            }
            rfvp::host_api::RfvpError::InvalidArgument => {
                crate::status::PsvStatus::InvalidArgument.as_i32()
            }
            rfvp::host_api::RfvpError::Unsupported => {
                crate::status::PsvStatus::Unsupported.as_i32()
            }
            rfvp::host_api::RfvpError::OutOfMemory => {
                crate::status::PsvStatus::OutOfMemory.as_i32()
            }
            rfvp::host_api::RfvpError::CapacityExceeded => {
                crate::status::PsvStatus::CapacityExceeded.as_i32()
            }
            rfvp::host_api::RfvpError::EndOfFile => crate::status::PsvStatus::EndOfFile.as_i32(),
            rfvp::host_api::RfvpError::Backend => crate::status::PsvStatus::Backend.as_i32(),
        },
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_push_quit(app: *mut PsvApp) -> i32 {
    if app.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    match app.push_event(RfvpEvent::Quit) {
        Ok(()) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => err_to_status(err),
    }
}

/// Keys the platform layer can press: 0 = Ctrl (skip), 1 = Return, 2 = Escape, 3 = Space.
#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_push_key(app: *mut PsvApp, key: u32, pressed: i32) -> i32 {
    if app.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }
    let key = match key {
        0 => KeyCode::Control,
        1 => KeyCode::Return,
        2 => KeyCode::Escape,
        3 => KeyCode::Space,
        _ => return crate::status::PsvStatus::InvalidArgument.as_i32(),
    };
    let app = unsafe { &mut *app };
    let event = if pressed != 0 {
        RfvpEvent::KeyDown {
            key,
            repeat: false,
            modifiers: InputModifiers::empty(),
        }
    } else {
        RfvpEvent::KeyUp {
            key,
            modifiers: InputModifiers::empty(),
        }
    };
    match app.push_event(event) {
        Ok(()) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => err_to_status(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_push_pointer_move(
    app: *mut PsvApp,
    x: i32,
    y: i32,
    in_screen: u8,
) -> i32 {
    if app.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    match app.push_event(RfvpEvent::PointerMove {
        x,
        y,
        in_screen: in_screen != 0,
    }) {
        Ok(()) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => err_to_status(err),
    }
}

fn err_to_status(err: rfvp::host_api::RfvpError) -> i32 {
    match err {
        rfvp::host_api::RfvpError::Io => crate::status::PsvStatus::Io.as_i32(),
        rfvp::host_api::RfvpError::NotFound => crate::status::PsvStatus::NotFound.as_i32(),
        rfvp::host_api::RfvpError::InvalidData => crate::status::PsvStatus::InvalidData.as_i32(),
        rfvp::host_api::RfvpError::InvalidArgument => {
            crate::status::PsvStatus::InvalidArgument.as_i32()
        }
        rfvp::host_api::RfvpError::Unsupported => crate::status::PsvStatus::Unsupported.as_i32(),
        rfvp::host_api::RfvpError::OutOfMemory => crate::status::PsvStatus::OutOfMemory.as_i32(),
        rfvp::host_api::RfvpError::CapacityExceeded => {
            crate::status::PsvStatus::CapacityExceeded.as_i32()
        }
        rfvp::host_api::RfvpError::EndOfFile => crate::status::PsvStatus::EndOfFile.as_i32(),
        rfvp::host_api::RfvpError::Backend => crate::status::PsvStatus::Backend.as_i32(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_push_pointer_down(
    app: *mut PsvApp,
    button: u32,
    x: i32,
    y: i32,
) -> i32 {
    if app.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    match app.push_event(RfvpEvent::PointerDown {
        button: raw_pointer_button(button),
        x,
        y,
    }) {
        Ok(()) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => err_to_status(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_push_pointer_up(
    app: *mut PsvApp,
    button: u32,
    x: i32,
    y: i32,
) -> i32 {
    if app.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    match app.push_event(RfvpEvent::PointerUp {
        button: raw_pointer_button(button),
        x,
        y,
    }) {
        Ok(()) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => err_to_status(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_push_wheel(
    app: *mut PsvApp,
    delta_x: i32,
    delta_y: i32,
) -> i32 {
    if app.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    match app.push_event(RfvpEvent::Wheel { delta_x, delta_y }) {
        Ok(()) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => err_to_status(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_app_push_touch(
    app: *mut PsvApp,
    phase: u32,
    id: u64,
    x: i32,
    y: i32,
) -> i32 {
    if app.is_null() {
        return crate::status::PsvStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    let event = match phase {
        0 => RfvpEvent::TouchDown { id, x, y },
        1 => RfvpEvent::TouchMove { id, x, y },
        2 => RfvpEvent::TouchUp { id, x, y },
        _ => return crate::status::PsvStatus::InvalidArgument.as_i32(),
    };
    match app.push_event(event) {
        Ok(()) => crate::status::PsvStatus::Ok.as_i32(),
        Err(err) => err_to_status(err),
    }
}

fn raw_pointer_button(button: u32) -> rfvp::host_api::PointerButton {
    match button {
        0 => rfvp::host_api::PointerButton::Left,
        1 => rfvp::host_api::PointerButton::Right,
        2 => rfvp::host_api::PointerButton::Middle,
        other => rfvp::host_api::PointerButton::Other(other as u16),
    }
}
