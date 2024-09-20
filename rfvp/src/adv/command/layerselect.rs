use super::prelude::*;
use crate::adv::vm_state::layers::LayerSelection;

impl StartableCommand for command::runtime::LAYERSELECT {
    fn apply_state(&self, state: &mut VmState) {
        let mut low = self.selection_start_id;
        let mut high = self.selection_end_id;

        if low > high {
            warn!(
                "LAYERSELECT: invalid selection range order: {:?} > {:?}",
                low, high
            );
            std::mem::swap(&mut low, &mut high);
        }

        state.layers.layer_selection = Some(LayerSelection { low, high })
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        _adv_state: &mut AdvState,
    ) -> CommandStartResult {
        self.token.finish().into()
    }
}
