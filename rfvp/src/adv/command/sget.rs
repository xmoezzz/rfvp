use rfvp_core::format::scenario::Scenario;

use super::prelude::*;

impl StartableCommand for command::runtime::SGET {
    fn apply_state(&self, _state: &mut VmState) {
        // nothing to do
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        _adv_state: &mut AdvState,
    ) -> CommandStartResult {
        let value = vm_state.persist.get(self.slot_number);
        self.token.finish(value).into()
    }
}
