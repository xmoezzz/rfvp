use super::prelude::*;

impl StartableCommand for command::runtime::PAGEBACK {
    fn apply_state(&self, _state: &mut VmState) {
        warn!("TODO: PAGEBACK state: {:?}", self);
        // TODO: I __think__ we should have a way to store this in the state
        // I am still not sure of the paradigm, lol
        // ignore for now (along with WIPE)
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        _adv_state: &mut AdvState,
    ) -> CommandStartResult {
        warn!("TODO: PAGEBACK: {:?}", self);
        self.token.finish().into()
    }
}
