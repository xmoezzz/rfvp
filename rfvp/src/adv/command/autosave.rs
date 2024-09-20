use super::prelude::*;

impl StartableCommand for command::runtime::AUTOSAVE {
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
        warn!("TODO: AUTOSAVE: {:?}", self);
        self.token.finish().into()
    }
}
