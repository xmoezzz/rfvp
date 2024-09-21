use std::fmt::{Debug, Formatter};

use rfvp_core::vm::command::types::AudioWaitStatus;

use super::prelude::*;

pub struct SEWAIT {
    token: Option<command::token::SEWAIT>,
    slot: i32,
    target_status: AudioWaitStatus,
}

impl StartableCommand for command::runtime::SEWAIT {
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
        Yield(
            SEWAIT {
                token: Some(self.token),
                slot: self.se_slot,
                target_status: self.target_status,
            }
            .into(),
        )
    }
}

impl UpdatableCommand for SEWAIT {
    fn update(
        &mut self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
        _is_fast_forwarding: bool,
    ) -> Option<CommandResult> {
        let status = adv_state.se_player.get_wait_status(self.slot);
        let finished = !(status & self.target_status).is_empty();

        if finished {
            Some(self.token.take().unwrap().finish())
        } else {
            None
        }
    }
}

impl Debug for SEWAIT {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SEWAIT")
            .field(&self.slot)
            .field(&self.target_status)
            .finish()
    }
}
