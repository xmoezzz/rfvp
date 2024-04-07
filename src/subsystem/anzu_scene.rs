use super::{resources::{prim::{PrimType, INVAILD_PRIM_HANDLE}, time::Time}, scene::Scene, world::World};
use crate::GameData;


#[derive(Default)]
pub struct AnzuScene {
}

impl Scene for AnzuScene {
    fn on_start(&mut self, data: &mut GameData) {
        data.add_default_camera();
    }
    
    fn on_update(&mut self, game_data: &mut GameData) {
        let frame_duration = game_data
            .get_resource_mut::<Time>()
            .expect("Time is an internal resource and can't be missing")
            .frame();
        
    }
}

impl AnzuScene {
    pub fn new() -> Self {
        Self {}
    }

    fn update_prim(&mut self, game_data: &mut GameData, elapsed: u64) {
        
    }

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

            let mut sprt_prim = game_data
                .motion_manager
                .prim_manager
                .get_prim(sprt);

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
            },
            PrimType::PrimTypeTile => {
                let tile = game_data
                    .motion_manager
                    .prim_manager
                    .get_prim(prim_id)
                    .get_tile();
                
                
            },
            PrimType::PrimTypeSprt => {},
            PrimType::PrimTypeText => {},
            PrimType::PrimTypeSnow => {},
            PrimType::PrimTypeNone => {},

        }

    }
}