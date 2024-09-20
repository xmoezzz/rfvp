use std::ops::Not;

use rfvp_core::{format::scenario::info::BgmInfoItem, time::Tween};

use super::prelude::*;
use crate::adv::vm_state::audio::BgmState;

impl StartableCommand for command::runtime::BGMPLAY {
    fn apply_state(&self, state: &mut VmState) {
        state.audio.bgm = self.no_repeat.not().then_some(BgmState {
            bgm_id: self.bgm_data_id,
            volume: self.volume,
        });
    }

    fn start(
        self,
        context: &UpdateContext,
        scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        let bgm_info @ BgmInfoItem {
            name: _,
            display_name,
            linked_bgm_id: _,
        } = scenario.info_tables().bgm_info(self.bgm_data_id);

        let audio = context
            .asset_server
            // TODO: sync - bad!!
            .load_sync(bgm_info.path())
            .expect("Failed to load BGM track");

        adv_state.bgm_player.play(
            audio,
            display_name.as_str(),
            !self.no_repeat,
            self.volume,
            Tween::linear(self.fade_in_time),
        );

        self.token.finish().into()
    }
}
