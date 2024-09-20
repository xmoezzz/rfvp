use std::ops::Not;

use rfvp_core::time::Tween;

use super::prelude::*;
use crate::adv::vm_state::audio::SeState;

impl StartableCommand for command::runtime::SEPLAY {
    fn apply_state(&self, state: &mut VmState) {
        state.audio.se[self.se_slot as usize] = self.no_repeat.not().then_some(SeState {
            se_id: self.se_data_id,
            volume: self.volume,
            pan: self.pan,
            play_speed: self.play_speed as f32 / 1000.0,
        });
    }

    fn start(
        self,
        context: &UpdateContext,
        scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        if self.play_speed != 1000 {
            warn!("TODO: SEPLAY: ignoring play_speed={}", self.play_speed);
        }

        let se_info = scenario.info_tables().se_info(self.se_data_id);

        let audio = context
            .asset_server
            // TODO: sync - bad!!
            .load_sync(se_info.path())
            .expect("Failed to load BGM track");

        adv_state.se_player.play(
            self.se_slot,
            audio,
            !self.no_repeat,
            self.volume,
            self.pan,
            Tween::linear(self.fade_in_time),
        );

        self.token.finish().into()
    }
}
