use std::fmt::{Debug, Formatter};

use super::prelude::*;

pub struct MSGSET {
    #[allow(unused)]
    token: Option<command::token::MSGSET>,
}

impl StartableCommand for command::runtime::MSGSET {
    fn apply_state(&self, state: &mut VmState) {
        // TODO: think about async messages (those where you would use MSGWAIT)
        state.messagebox_state.text = Some(self.text.clone());
        state.messagebox_state.messagebox_shown = true;
    }

    fn start(
        self,
        context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        adv_state
            .root_layer_group
            .message_layer_mut()
            .set_message(context, &self.text);

        if self.auto_wait {
            Yield(
                MSGSET {
                    token: Some(self.token),
                }
                .into(),
            )
        } else {
            self.token.finish().into()
        }
    }
}

impl UpdatableCommand for MSGSET {
    fn update(
        &mut self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
        // TODO: I think we __should__ somehow to react to fast-forwarding here
        // rn it's kludged in the ADV update function though
        _is_fast_forwarding: bool,
    ) -> Option<CommandResult> {
        if adv_state.root_layer_group.message_layer().is_finished() {
            Some(self.token.take().unwrap().finish())
        } else {
            None
        }
    }
}

impl Debug for MSGSET {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MSGSET").finish()
    }
}
