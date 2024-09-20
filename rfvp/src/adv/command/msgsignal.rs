use super::prelude::*;

impl StartableCommand for command::runtime::MSGSIGNAL {
    fn apply_state(&self, _state: &mut VmState) {
        // nothing to do
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        adv_state.root_layer_group.message_layer_mut().signal();
        self.token.finish().into()
    }
}
