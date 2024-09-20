use rfvp_core::time::Tween;

use super::prelude::*;

impl StartableCommand for command::runtime::SEPAN {
    fn apply_state(&self, state: &mut VmState) {
        if let Some(state) = state.audio.se[self.se_slot as usize].as_mut() {
            state.pan = self.pan;
        }
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        adv_state
            .se_player
            .set_panning(self.se_slot, self.pan, Tween::linear(self.fade_in_time));

        self.token.finish().into()
    }
}
