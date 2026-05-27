use rfvp::host_api::{RfvpEvent, RfvpResult};
use rfvp::{RfvpCore, RfvpCoreConfig, RfvpTickResult};

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
        Ok(Self {
            core: RfvpCore::new(config),
            host: HorizonHost::new(),
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

    pub fn push_event(&mut self, event: RfvpEvent) -> RfvpResult<()> {
        self.events.push(event)
    }

    pub fn poll_platform_events(&mut self) -> RfvpResult<()> {
        self.input.poll(&mut self.events)
    }

    pub fn tick(&mut self) -> RfvpResult<RfvpTickResult> {
        self.poll_platform_events()?;
        for event in self.events.as_slice().iter().copied() {
            self.core.push_event(event)?;
        }
        self.events.clear();
        self.core.tick(&mut self.host)
    }

    pub fn render_empty_frame(&mut self) -> RfvpResult<()> {
        self.core.render_empty_frame(&mut self.host)
    }

    pub fn run_empty_frame(&mut self) -> RfvpResult<RfvpTickResult> {
        let result = self.tick()?;
        self.render_empty_frame()?;
        Ok(result)
    }

    pub fn quit_requested(&self) -> bool {
        self.core.quit_requested()
    }
}
