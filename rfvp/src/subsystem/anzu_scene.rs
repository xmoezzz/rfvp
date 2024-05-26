use super::{
    resources::{
        color_manager::ColorItem,
        prim::{PrimType, INVAILD_PRIM_HANDLE},
    },
    scene::Scene,
    world::{GameData},
};
use crate::script::global::get_int_var;

#[derive(Default)]
pub struct AnzuScene {}

impl Scene for AnzuScene {
    fn on_start(&mut self, _data: &mut GameData) {
    }

    fn on_update(&mut self, game_data: &mut GameData) {
        let frame_duration = game_data
            .time()
            .frame()
            .as_millis() as i64;

        self.update_alpha_motions(game_data, frame_duration);
        self.update_move_motions(game_data, frame_duration);
        self.update_rotation_motions(game_data, frame_duration);
        self.update_scale_motions(game_data, frame_duration);
        self.update_z_motions(game_data, frame_duration);
        self.update_v3d_motions(game_data, frame_duration);
    }
}

impl AnzuScene {
    pub fn new() -> Self {
        Self {}
    }

    fn update_alpha_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_alpha_motions(elapsed, true);
    }

    fn update_move_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_move_motions(elapsed, true);
    }

    fn update_scale_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data
            .motion_manager
            .update_s2_move_motions(elapsed, true);
    }

    fn update_rotation_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data
            .motion_manager
            .update_rotation_motions(elapsed, true);
    }

    fn update_z_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_z_motions(elapsed, true);
    }

    fn update_v3d_motions(&mut self, game_data: &mut GameData, elapsed: i64) {
        game_data.motion_manager.update_v3d_motions(elapsed, true);
    }

    fn update_prim(&mut self, _game_data: &mut GameData, _elapsed: u64) {}

    fn draw_prim_container(&mut self, game_data: &mut GameData, prim_id: i16, x: i32, y: i32) {
        let draw_flag = game_data
            .motion_manager
            .prim_manager
            .get_prim(prim_id)
            .get_draw_flag();

        if !draw_flag {
            return;
        }

        let mut prim_id = prim_id;
        loop {
            let sprt = game_data
                .motion_manager
                .prim_manager
                .get_prim(prim_id)
                .get_sprt();
            if sprt == INVAILD_PRIM_HANDLE {
                break;
            }

            let prim_x = game_data
                .motion_manager
                .prim_manager
                .get_prim(prim_id)
                .get_x();

            let prim_y = game_data
                .motion_manager
                .prim_manager
                .get_prim(prim_id)
                .get_y();

            let prim_alpha = game_data
                .motion_manager
                .prim_manager
                .get_prim(prim_id)
                .get_alpha();

            let mut sprt_prim = game_data.motion_manager.prim_manager.get_prim(sprt);

            sprt_prim.set_x(prim_x);
            sprt_prim.set_y(prim_y);
            sprt_prim.set_alpha(prim_alpha);
            prim_id = sprt;
            if !sprt_prim.get_draw_flag() {
                return;
            }
        }

        let typ = game_data
            .motion_manager
            .prim_manager
            .get_prim(prim_id)
            .get_type();

        match typ {
            PrimType::PrimTypeGroup => {
                let mut i = game_data
                    .motion_manager
                    .prim_manager
                    .get_prim(prim_id)
                    .get_child();

                while i != INVAILD_PRIM_HANDLE {
                    let prim_x = game_data
                        .motion_manager
                        .prim_manager
                        .get_prim(prim_id)
                        .get_x() as i32;

                    let prim_y = game_data
                        .motion_manager
                        .prim_manager
                        .get_prim(prim_id)
                        .get_y() as i32;

                    self.draw_prim_container(game_data, i, x + prim_x, y + prim_y);
                    i = game_data
                        .motion_manager
                        .prim_manager
                        .get_prim(i)
                        .get_grand_son();
                }
            }
            PrimType::PrimTypeTile => {
                let tile = game_data
                    .motion_manager
                    .prim_manager
                    .get_prim(prim_id)
                    .get_tile();

                let mut color = ColorItem::new();
                color.set_r(get_int_var(5) as u8);
                color.set_g(get_int_var(6) as u8);
                color.set_b(get_int_var(7) as u8);
                color.set_a(get_int_var(8) as u8);

                if tile != -1 {
                    color = game_data.color_manager.get_entry(tile as u8).clone();
                }
            }
            PrimType::PrimTypeSprt => {}
            PrimType::PrimTypeText => {}
            PrimType::PrimTypeSnow => {}
            PrimType::PrimTypeNone => {}
        }
    }
}
