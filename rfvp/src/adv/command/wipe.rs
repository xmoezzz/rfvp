use super::prelude::*;

impl StartableCommand for command::runtime::WIPE {
    fn apply_state(&self, _state: &mut VmState) {
        warn!("TODO: WIPE state: {:?}", self);
        // we don't track wipes yet
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        _adv_state: &mut AdvState,
    ) -> CommandStartResult {
        warn!("TODO: WIPE: {:?}", self);
        self.token.finish().into()
    }
}
