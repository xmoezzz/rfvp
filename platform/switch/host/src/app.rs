use rfvp::host_api::{RfvpEvent, RfvpResult};
use rfvp::{RfvpBootConfig, RfvpCore, RfvpCoreConfig, RfvpTickResult};

use crate::event::{HorizonEventQueue, HorizonInput};
use crate::host::HorizonHost;

pub struct HorizonApp {
    core: RfvpCore,
    host: HorizonHost,
    events: HorizonEventQueue,
    input: HorizonInput,
}

impl HorizonApp {
    pub fn new(config: RfvpCoreConfig, event_capacity: usize) -> RfvpResult<Self> {
        Self::with_game_root(config, event_capacity, crate::fs::DEFAULT_GAME_ROOT)
    }

    pub fn with_game_root(
        config: RfvpCoreConfig,
        event_capacity: usize,
        game_root: &str,
    ) -> RfvpResult<Self> {
        Ok(Self {
            core: RfvpCore::new(config),
            host: HorizonHost::with_game_root(game_root),
            events: HorizonEventQueue::new(event_capacity),
            input: HorizonInput::new()?,
        })
    }

    pub fn core(&self) -> &RfvpCore {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut RfvpCore {
        &mut self.core
    }

    pub fn host(&mut self) -> &mut HorizonHost {
        &mut self.host
    }

    /// Load the game: find the script, open the resource packs and start the VM.
    pub fn boot(&mut self, config: RfvpBootConfig<'_>) -> RfvpResult<()> {
        self.core.boot(&mut self.host, config)
    }

    pub fn push_event(&mut self, event: RfvpEvent) -> RfvpResult<()> {
        self.events.push(event)
    }

    pub fn poll_platform_events(&mut self) -> RfvpResult<()> {
        self.input.poll(&mut self.events)
    }

    /// Pointer positions arrive in screen pixels; the game wants its own virtual coordinates
    /// (the screen shows the virtual frame scaled and letterboxed).
    fn to_virtual(&self, event: RfvpEvent) -> RfvpEvent {
        let map = |x: i32, y: i32| {
            let (vx, vy) = self
                .host
                .renderer_ref()
                .screen_to_virtual(x as f32, y as f32);
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

    /// Run one frame: deliver input, advance the script and draw. The core draws and presents
    /// the frame itself, which also paces the loop to the display.
    pub fn tick(&mut self) -> RfvpResult<RfvpTickResult> {
        self.poll_platform_events()?;
        for index in 0..self.events.as_slice().len() {
            let event = self.to_virtual(self.events.as_slice()[index]);
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
