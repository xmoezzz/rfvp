use crate::script::Variant;

use super::{scene::Scene, world::GameData};
use crate::{script::global::GLOBAL, subsystem::resources::input_manager::KeyCode};

#[derive(Default)]
pub struct AnzuScene {}

impl Scene for AnzuScene {
    fn on_start(&mut self, _data: &mut GameData) {}

    fn on_update(&mut self, game_data: &mut GameData) {
        let frame_duration = game_data.time_mut_ref().delta_duration();
        let frame_us = frame_duration.as_micros() as i64;
        let frame_ms = ((frame_us as u64) + 999) / 1000;
        let frame_duration = frame_ms as i64;

        crate::trace::vm(format_args!(
            "AnzuScene::on_update frame_duration={}",
            frame_duration
        ));

        // --- ControlPulse semantics (from original engine) ---
        //
        // Original update loop:
        //   if (Ctrl is held) || (scene.control_is_pulse) {
        //     elapsed = -elapsed;
        //     scene.control_is_pulse = 0;
        //   }
        // A negative elapsed is a "fast-forward" signal: most motion containers treat
        // elapsed < 0 as "commit final state immediately".
        // Text reveal is handled separately, but it uses the same Ctrl/ControlPulse condition.
        let ctrl_down =
            (game_data.inputs_manager.get_input_state() & (1u32 << (KeyCode::Ctrl as u32))) != 0;
        let pulse = game_data.inputs_manager.peek_control_pulse();
        let fast_forward = ctrl_down || pulse;
        let elapsed = if fast_forward {
            -frame_duration
        } else {
            frame_duration
        };

        self.update_alpha_motions(game_data, elapsed);
        self.update_move_motions(game_data, elapsed);
        self.update_rotation_motions(game_data, elapsed);
        self.update_scale_motions(game_data, elapsed);
        self.update_z_motions(game_data, elapsed);
        self.update_v3d_motions(game_data, elapsed);
        self.update_anim_motions(game_data, elapsed);
        self.update_parts_motions(game_data, elapsed);
        self.update_snow_motions(game_data, elapsed);
        self.update_dissolve(game_data, frame_duration, fast_forward);
    }

    fn late_update(&mut self, game_data: &mut GameData) {
        // Text reveal and its repaint/upload must observe the effects of VM syscalls issued
        // in this frame (TextPrint/TextRepaint/ControlPulse). Therefore we update it in
        // late_update, after scheduler.execute().
        let frame_duration = game_data.time_mut_ref().delta_duration();
        let frame_us = frame_duration.as_micros() as i64;
        let frame_ms = ((frame_us as u64) + 999) / 1000;
        let frame_duration = frame_ms as i64;

        let ctrl_down =
            (game_data.inputs_manager.get_input_state() & (1u32 << (KeyCode::Ctrl as u32))) != 0;
        let pulse = game_data.inputs_manager.take_control_pulse();
        let fast_forward = ctrl_down || pulse;
        let elapsed = if fast_forward {
            -frame_duration
        } else {
            frame_duration
        };

        self.update_text_reveal(game_data, elapsed);
    }

}

impl AnzuScene {
    pub fn new() -> Self {
        Self {}
    }

    fn update_alpha_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data
            .motion_manager
            .update_alpha_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_move_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data
            .motion_manager
            .update_move_motions(elapsed, game_data.get_game_should_exit());
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
        game_data
            .motion_manager
            .update_z_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_v3d_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data
            .motion_manager
            .update_v3d_motions(elapsed, game_data.get_game_should_exit());
    }

    fn update_anim_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_anim_motions(elapsed);
        let bgm_playing_slots = game_data.bgm_player.get_playing_slots();
        game_data
            .motion_manager
            .update_lip_motions(elapsed,  game_data.get_game_should_exit(), &bgm_playing_slots);
    }

    fn update_parts_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_parts_motions(elapsed);
    }

    fn update_snow_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        let w = game_data.get_width() as i32;
        let h = game_data.get_height() as i32;
        game_data.motion_manager.update_snow_motions(elapsed, w, h);
    }

    fn update_text_reveal(&mut self, game_data: &mut GameData, elapsed: i64) {
        // Reverse-engineered: update_text uses get_var(&engine->scene, 0), which reads
        // Scene.var_tbl[non_volatile_global_count + 0] and returns 0 unless the slot is Int.
        // In our Global implementation, this corresponds to Global::get_int_var(0), NOT Global::get(0).
        let global_speed_var0 = GLOBAL.lock().unwrap().get_int_var(0);
        let release_special_wait = elapsed < 0;
        game_data
            .motion_manager
            .update_text_reveal(elapsed, global_speed_var0, release_special_wait, &game_data.fontface_manager);
    }

    fn update_dissolve(&mut self, game_data: &mut GameData, elapsed: i64, fast_forward: bool) {
        // Dissolve progression is global (not per-prim).
        //
        // Ctrl/ControlPulse fast-forward: for dissolve, the intended observable behavior is
        // "finish quickly" so that DISSOLVE_WAIT can unblock.
        if fast_forward {
            game_data.motion_manager.tick_dissolve(u32::MAX);
            game_data.motion_manager.tick_dissolve2(u32::MAX);
            return;
        }
        if elapsed <= 0 {
            return;
        }

        let ms = elapsed as u32;
        game_data.motion_manager.tick_dissolve(ms);
        game_data.motion_manager.tick_dissolve2(ms);
    }

    fn update_prim(&mut self, _game_data: &mut GameData, _elapsed: u64) {}
}
