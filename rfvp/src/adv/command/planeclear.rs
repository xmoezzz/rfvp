use super::prelude::*;

impl StartableCommand for command::runtime::PLANECLEAR {
    fn apply_state(&self, state: &mut VmState) {
        let plane = &mut state.layers.planes[state.layers.current_plane as usize];
        plane.layers.clear();
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        let layer_group = adv_state.current_plane_layer_group_mut(vm_state);
        for layer_id in layer_group.get_layer_ids().collect::<Vec<_>>() {
            layer_group.remove_layer(layer_id);
        }

        self.token.finish().into()
    }
}
