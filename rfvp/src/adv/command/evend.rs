use super::prelude::*;

impl StartableCommand for command::runtime::EVEND {
    fn apply_state(&self, _state: &mut VmState) {
        warn!("TODO: EVEND state: {:?}", self);
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        _adv_state: &mut AdvState,
    ) -> CommandStartResult {
        warn!("TODO: EVEND: {:?}", self);
        self.token.finish().into()
    }
}
