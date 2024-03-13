use anyhow::{bail, Result};

use crate::subsystem::resources::{graph_buff::GraphBuff, motion_manager::DissolveType};
use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

// UNUSED macro
macro_rules! UNUSED {
    ($($x:ident),*) => {
        $(let _ = $x;)*
    };
}

#[allow(clippy::too_many_arguments)]
pub fn lip_anim(
    game_data: &mut GameData,
    id: &Variant,
    typ: &Variant,
    id2: &Variant,
    duration: &Variant,
    id3: &Variant,
    duration2: &Variant,
    id4: &Variant,
    duration3: &Variant,
) -> Result<Variant> {
    UNUSED!(game_data, id, typ, id2, duration, id3, duration2, id4, duration3);
    log::error!("lip_anim: not implemented");
    Ok(Variant::Nil)
}

pub fn lip_sync(game_data: &mut GameData, id: &Variant, sync: &Variant) -> Result<Variant> {
    UNUSED!(game_data, id, sync);
    log::error!("lip_sync: not implemented");
    Ok(Variant::Nil)
}

#[allow(clippy::too_many_arguments)]
pub fn dissolve(
    game_data: &mut GameData,
    duration: &Variant,
    name_or_color: &Variant,
    inout: &Variant,
    x: &Variant,
    y: &Variant,
    w: &Variant,
    h: &Variant,
) -> Result<Variant> {
    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => bail!("dissolve: invalid duration type"),
    };

    if !(1..=300000).contains(&duration) {
        bail!("dissolve: duration should be in range 1..300000");
    }

    let game_width = game_data.get_width() as i16;
    let game_height = game_data.get_height() as i16;

    match name_or_color {
        Variant::ConstString(s, _) | Variant::String(s) => {
            let buff = game_data.vfs_load_file(s)?;
            let mut graph = GraphBuff::new();
            graph.load_mask(s, buff)?;
            game_data.motion_manager.set_dissolve_mask_graph(graph);
            if inout.is_true() {
                
            }
        },
        Variant::Int(color_id) => {
            let color_id = *color_id;
            if color_id >= 1 && color_id <= 255 {
                game_data.motion_manager.set_dissolve_type(DissolveType::ColoredFadeOut);
                game_data.motion_manager.set_dissolve_color_id(color_id as u32);
                let mask_prim = game_data.motion_manager.get_mask_prim();
                mask_prim.set_x(0);
                mask_prim.set_y(0);
                mask_prim.set_w(game_width);
                mask_prim.set_h(game_height);
                if let Variant::Int(x) = x {
                    mask_prim.set_x(*x as i16);
                }
                if let Variant::Int(y) = y {
                    mask_prim.set_y(*y as i16);
                }
                if let Variant::Int(w) = w {
                    mask_prim.set_w(*w as i16);
                }
                if let Variant::Int(h) = h {
                    mask_prim.set_h(*h as i16);
                }
            }
        },
        _ => {
            game_data.motion_manager.set_dissolve_type(DissolveType::ColoredFadeIn);
        },
    }
    Ok(Variant::Nil)
}
