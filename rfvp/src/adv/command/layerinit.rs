use super::prelude::*;

impl StartableCommand for command::runtime::LAYERINIT {
    fn apply_state(&self, state: &mut VmState) {
        state
            .layers
            .get_vlayer_mut(self.layer_id)
            .for_each(|layer| layer.properties.init());
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        adv_state
            .get_vlayer_mut(vm_state, self.layer_id)
            .for_each(|mut layer| layer.properties_mut().init());
        self.token.finish().into()
    }
}
