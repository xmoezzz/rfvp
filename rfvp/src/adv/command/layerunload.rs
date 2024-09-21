use tracing::debug;

use super::prelude::*;

impl StartableCommand for command::runtime::LAYERUNLOAD {
    fn apply_state(&self, state: &mut VmState) {
        state.layers.get_vlayer_ids(self.layer_id).for_each(|id| {
            state.layers.free(id);
        });
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        vm_state
            .layers
            .get_vlayer_ids(self.layer_id)
            .for_each(|id| {
                debug!("Unloading {:?}", id);
                adv_state
                    .current_plane_layer_group_mut(vm_state)
                    .remove_layer(id);
            });
        self.token.finish().into()
    }
}
