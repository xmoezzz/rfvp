use std::fmt::{Debug, Formatter};

use super::prelude::*;

pub struct MSGWAIT {
    token: Option<command::token::MSGWAIT>,
    section_num: i32,
}

impl StartableCommand for command::runtime::MSGWAIT {
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
            MSGWAIT {
                token: Some(self.token),
                section_num: self.section_num,
            }
            .into(),
        )
    }
}

impl UpdatableCommand for MSGWAIT {
    fn update(
        &mut self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
        _is_fast_forwarding: bool,
    ) -> Option<CommandResult> {
        let message_layer = adv_state.root_layer_group.message_layer();

        let finished = if self.section_num == -1 {
            // wait for the whole message to complete
            message_layer.is_finished()
        } else {
            message_layer.is_section_finished(self.section_num as u32)
        };

        if finished {
            Some(self.token.take().unwrap().finish())
        } else {
            None
        }
    }
}

impl Debug for MSGWAIT {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MSGWAIT").field(&self.section_num).finish()
    }
}
