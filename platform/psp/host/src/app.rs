use alloc::boxed::Box;

use rfvp::host_api::{InputModifiers, KeyCode, PointerButton, RfvpEvent, RfvpResult};
use rfvp::{RfvpBootConfig, RfvpCore, RfvpCoreConfig, RfvpTickResult};

use crate::event::PspEventQueue;
use crate::host::PspHost;
use crate::raw::RawPspHost;
use crate::status::PspStatus;
use crate::viewport::PspViewport;

pub struct PspApp {
    core: RfvpCore,
    host: PspHost,
    events: PspEventQueue,
    viewport: PspViewport,
}

impl PspApp {
    pub fn new(
        raw_host: RawPspHost,
        config: RfvpCoreConfig,
        event_capacity: usize,
        target_width: u32,
        target_height: u32,
    ) -> Self {
        Self {
            core: RfvpCore::new(config),
            host: PspHost::from_raw(raw_host),
            events: PspEventQueue::new(event_capacity),
            viewport: PspViewport::new(
                config.virtual_width,
                config.virtual_height,
                target_width,
                target_height,
            ),
        }
    }

    pub fn boot_old_school(&mut self, config: RfvpBootConfig<'_>) -> RfvpResult<()> {
        self.core.boot(&mut self.host, config)?;
        let core_config = self.core.config();
        self.viewport = PspViewport::new(
            core_config.virtual_width,
            core_config.virtual_height,
            self.viewport.target_w,
            self.viewport.target_h,
        );
        Ok(())
    }

    pub fn push_event(&mut self, event: RfvpEvent) -> RfvpResult<()> {
        self.events.push(event)
    }

    pub fn push_pointer_move_physical(&mut self, x: i32, y: i32) -> RfvpResult<()> {
        let (x, y, in_screen) = self.viewport.physical_to_logical(x, y);
        self.push_event(RfvpEvent::PointerMove { x, y, in_screen })
    }

    pub fn push_pointer_down_physical(
        &mut self,
        button: PointerButton,
        x: i32,
        y: i32,
    ) -> RfvpResult<()> {
        let (x, y, _) = self.viewport.physical_to_logical(x, y);
        self.push_event(RfvpEvent::PointerDown { button, x, y })
    }

    pub fn push_pointer_up_physical(
        &mut self,
        button: PointerButton,
        x: i32,
        y: i32,
    ) -> RfvpResult<()> {
        let (x, y, _) = self.viewport.physical_to_logical(x, y);
        self.push_event(RfvpEvent::PointerUp { button, x, y })
    }

    pub fn push_key(&mut self, key: KeyCode, pressed: bool) -> RfvpResult<()> {
        let event = if pressed {
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
        self.push_event(event)
    }

    pub fn tick(&mut self) -> RfvpResult<RfvpTickResult> {
        for event in self.events.as_slice().iter().copied() {
            self.core.push_event(event)?;
        }
        self.events.clear();
        self.core.tick(&mut self.host)
    }

    pub fn run_frame(&mut self) -> RfvpResult<RfvpTickResult> {
        self.tick()
    }

    pub fn quit_requested(&self) -> bool {
        self.core.quit_requested()
    }
}

#[no_mangle]
pub extern "C" fn rfvp_psp_app_create(
    raw_host: RawPspHost,
    virtual_width: u32,
    virtual_height: u32,
    target_width: u32,
    target_height: u32,
    max_pending_events: usize,
    event_capacity: usize,
) -> *mut PspApp {
    let config = RfvpCoreConfig {
        virtual_width,
        virtual_height,
        max_pending_events,
    };
    Box::into_raw(Box::new(PspApp::new(
        raw_host,
        config,
        event_capacity,
        target_width,
        target_height,
    )))
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psp_app_destroy(app: *mut PspApp) {
    if !app.is_null() {
        unsafe {
            drop(Box::from_raw(app));
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psp_app_push_quit(app: *mut PspApp) -> i32 {
    if app.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    match app.push_event(RfvpEvent::Quit) {
        Ok(()) => PspStatus::Ok.as_i32(),
        Err(err) => crate::status::rfvp_error_to_status(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psp_app_push_key(app: *mut PspApp, key_id: u32, pressed: i32) -> i32 {
    if app.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let Some(key) = key_from_psp_id(key_id) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    let app = unsafe { &mut *app };
    match app.push_key(key, pressed != 0) {
        Ok(()) => PspStatus::Ok.as_i32(),
        Err(err) => crate::status::rfvp_error_to_status(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psp_app_push_pointer_move(app: *mut PspApp, x: i32, y: i32) -> i32 {
    if app.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let app = unsafe { &mut *app };
    match app.push_pointer_move_physical(x, y) {
        Ok(()) => PspStatus::Ok.as_i32(),
        Err(err) => crate::status::rfvp_error_to_status(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_psp_app_push_pointer_button(
    app: *mut PspApp,
    button_id: u32,
    pressed: i32,
    x: i32,
    y: i32,
) -> i32 {
    if app.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let button = match button_id {
        0 => PointerButton::Left,
        1 => PointerButton::Right,
        2 => PointerButton::Middle,
        other => PointerButton::Other(other as u16),
    };
    let app = unsafe { &mut *app };
    let result = if pressed != 0 {
        app.push_pointer_down_physical(button, x, y)
    } else {
        app.push_pointer_up_physical(button, x, y)
    };
    match result {
        Ok(()) => PspStatus::Ok.as_i32(),
        Err(err) => crate::status::rfvp_error_to_status(err),
    }
}

fn key_from_psp_id(key_id: u32) -> Option<KeyCode> {
    Some(match key_id {
        1 => KeyCode::Return,
        2 => KeyCode::Escape,
        3 => KeyCode::Space,
        4 => KeyCode::Backspace,
        5 => KeyCode::Left,
        6 => KeyCode::Right,
        7 => KeyCode::Up,
        8 => KeyCode::Down,
        9 => KeyCode::PageUp,
        10 => KeyCode::PageDown,
        other => KeyCode::Unknown(other),
    })
}
