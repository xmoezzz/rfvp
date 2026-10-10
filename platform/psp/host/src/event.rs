use alloc::vec::Vec;

use rfvp::host_api::{RfvpError, RfvpEvent, RfvpResult};

pub struct PspEventQueue {
    events: Vec<RfvpEvent>,
    capacity: usize,
}

impl PspEventQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: Vec::new(),
            capacity,
        }
    }

    pub fn push(&mut self, event: RfvpEvent) -> RfvpResult<()> {
        if self.events.len() >= self.capacity {
            return Err(RfvpError::CapacityExceeded);
        }
        self.events.push(event);
        Ok(())
    }

    pub fn as_slice(&self) -> &[RfvpEvent] {
        &self.events
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}
