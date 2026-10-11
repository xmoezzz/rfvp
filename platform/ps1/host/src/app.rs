use rfvp::host_api::{RfvpEvent, RfvpResult};
use rfvp::{RfvpBootConfig, RfvpCore, RfvpCoreConfig, RfvpTickResult};

use crate::event::{PadInput, Ps1Input};
use crate::host::Ps1Host;

pub struct Ps1App {
    core: RfvpCore,
    host: Ps1Host,
    events: alloc::vec::Vec<RfvpEvent>,
    event_capacity: usize,
    input: PadInput,
}

impl Ps1App {
    pub fn new(config: RfvpCoreConfig, event_capacity: usize) -> Self {
        Self {
            core: RfvpCore::new(config),
            host: Ps1Host::new(),
            events: alloc::vec::Vec::new(),
            event_capacity,
            input: PadInput::new(),
        }
    }

    pub fn core(&self) -> &RfvpCore {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut RfvpCore {
        &mut self.core
    }

    pub fn host(&mut self) -> &mut Ps1Host {
        &mut self.host
    }

    /// Load the game: find the script, open the resource packs and start the VM.
    pub fn boot(&mut self, config: RfvpBootConfig<'_>) -> RfvpResult<()> {
        self.core.boot(&mut self.host, config)
    }

    /// Pointer positions arrive in screen pixels; the game wants its own virtual coordinates
    /// (the screen shows the virtual frame scaled and letterboxed).
    fn to_virtual(&self, event: RfvpEvent) -> RfvpEvent {
        let renderer = self.host.renderer_ref();
        let screen_h = renderer.screen_size().1 as f32;
        let map = |x: i32, y: i32| {
            let y = y as f32 * screen_h / crate::event::SCREEN_HEIGHT as f32;
            let (vx, vy) = renderer.screen_to_virtual(x as f32, y);
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

    /// Run one frame: deliver this frame's input, advance the script and draw.
    pub fn run_frame(&mut self, input: &Ps1Input) -> RfvpResult<RfvpTickResult> {
        self.events.clear();
        self.input
            .poll(input.buttons, input.stick_x, input.stick_y, &mut self.events);
        self.events.truncate(self.event_capacity);
        for index in 0..self.events.len() {
            let event = self.to_virtual(self.events[index]);
            self.core.push_event(event)?;
        }
        self.core.tick(&mut self.host)
    }

    /// The pointer cursor in frame pixels, when the stick has shown it.
    pub fn cursor(&self) -> Option<(i32, i32)> {
        self.input.cursor_shown().then(|| self.input.cursor())
    }

    pub fn quit_requested(&self) -> bool {
        self.core.quit_requested()
    }
}
