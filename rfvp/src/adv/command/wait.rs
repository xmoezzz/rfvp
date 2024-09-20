use std::fmt::{Debug, Formatter};

use tracing::debug;

use super::prelude::*;
use crate::update::UpdateContext;

pub struct WAIT {
    token: Option<command::token::WAIT>,
    waiting_left: Ticks,
}

impl StartableCommand for command::runtime::WAIT {
    fn apply_state(&self, _state: &mut VmState) {
        // nothing to do
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        _adv_state: &mut AdvState,
    ) -> CommandStartResult {
        assert!(!self.allow_interrupt);
        Yield(
            WAIT {
                token: Some(self.token),
                waiting_left: self.wait_amount,
            }
            .into(),
        )
    }
}

impl UpdatableCommand for WAIT {
    fn update(
        &mut self,
        context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        _adv_state: &mut AdvState,
        is_fast_forwarding: bool,
    ) -> Option<CommandResult> {
        self.waiting_left -= context.time_delta_ticks();
        // TODO: short circuit the wait for now
        if self.waiting_left <= Ticks::ZERO || is_fast_forwarding {
            debug!("WAIT: done");
            // TODO: this is kinda boilerplaty, maybe we want to have a TokenCell type?
            Some(self.token.take().unwrap().finish())
        } else {
            None
        }
    }
}

impl Debug for WAIT {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WAIT").field(&self.waiting_left).finish()
    }
}
