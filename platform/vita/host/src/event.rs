use alloc::vec::Vec;

use rfvp::host_api::{RfvpError, RfvpEvent, RfvpResult};

pub struct PsvEventQueue {
    events: Vec<RfvpEvent>,
    capacity: usize,
}

impl PsvEventQueue {
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

    pub fn drain_into<F>(&mut self, mut sink: F) -> RfvpResult<usize>
    where
        F: FnMut(RfvpEvent) -> RfvpResult<()>,
    {
        let count = self.events.len();
        for event in self.events.drain(..) {
            sink(event)?;
        }
        Ok(count)
    }

    pub fn as_slice(&self) -> &[RfvpEvent] {
        &self.events
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}
