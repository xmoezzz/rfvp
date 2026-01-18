use super::{scene::Scene, world::GameData};

#[derive(Default)]
pub struct AnzuScene {}

impl Scene for AnzuScene {
    fn on_start(&mut self, _data: &mut GameData) {
    }

    fn on_update(&mut self, game_data: &mut GameData) {
        let frame_duration = game_data.time_mut_ref().delta_duration();
        let frame_us = frame_duration.as_micros() as i64;
        let frame_ms = ((frame_us as u64) + 999) / 1000;
        let frame_duration = frame_ms as i64;

        println!("AnzuScene: on_update called with frame_duration {}", frame_duration);

        self.update_alpha_motions(game_data, frame_duration);
        self.update_move_motions(game_data, frame_duration);
        self.update_rotation_motions(game_data, frame_duration);
        self.update_scale_motions(game_data, frame_duration);
        self.update_z_motions(game_data, frame_duration);
        self.update_v3d_motions(game_data, frame_duration);
        self.update_anim_motions(game_data, frame_duration);
        self.update_snow_motions(game_data, frame_duration);
        self.update_text_reveal(game_data, frame_duration);
        self.update_dissolve(game_data, frame_duration);
    }
}

impl AnzuScene {
    pub fn new() -> Self {
        Self {}
    }

    fn update_alpha_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_alpha_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_move_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_move_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_scale_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data
            .motion_manager
            .update_s2_move_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_rotation_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data
            .motion_manager
            .update_rotation_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_z_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_z_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_v3d_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_v3d_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_anim_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_anim_motions(elapsed);
    }

    fn update_snow_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        let w = game_data.get_width() as i32;
        let h = game_data.get_height() as i32;
        game_data.motion_manager.update_snow_motions(elapsed, w, h);
    }

    fn update_text_reveal(&mut self, game_data: &mut GameData, elapsed: i64) {
        // Text reveal and upload runs from the same tick as other motions.
        game_data
            .motion_manager
            .update_text_reveal(elapsed, &game_data.fontface_manager);
    }

    fn update_dissolve(&mut self, game_data: &mut GameData, elapsed: i64) {
        // Dissolve progression is global (not per-prim).
        if elapsed <= 0 {
            return;
        }
        game_data.motion_manager.tick_dissolve(elapsed as u32);
    }


    fn update_prim(&mut self, _game_data: &mut GameData, _elapsed: u64) {}

}
