use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::resources::{graph_buff::GraphBuff, motion_manager::DissolveType};
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
            if inout.is_true() {}
        }
        Variant::Int(color_id) => {
            let color_id = *color_id;
            if (1..=255).contains(&color_id) {
                game_data
                    .motion_manager
                    .set_dissolve_type(DissolveType::ColoredFadeOut);
                game_data
                    .motion_manager
                    .set_dissolve_color_id(color_id as u32);
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
        }
        _ => {
            game_data
                .motion_manager
                .set_dissolve_type(DissolveType::ColoredFadeIn);
        }
    }
    Ok(Variant::Nil)
}


#[allow(clippy::too_many_arguments)]
pub fn snow(
    game_data: &mut GameData,
    id: &Variant,
    width: &Variant,
    height: &Variant,
    arg3: &Variant,
    arg4: &Variant,
    arg5: &Variant,
    arg6: &Variant,
    arg7: &Variant,
    arg8: &Variant,
    arg9: &Variant,
    arg10: &Variant,
    arg11: &Variant,
    arg12: &Variant,
    arg13: &Variant,
    arg14: &Variant,
    arg15: &Variant,
    arg16: &Variant,
    arg17: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::warn!("snow: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..=1).contains(&id) {
        log::warn!("snow: id should be in range 0..1");
        return Ok(Variant::Nil);
    }

    let width = match width {
        Variant::Int(width) => *width,
        _ => bail!("snow: invalid width type"),
    };

    if !(0..=4096).contains(&width) {
        bail!("snow: width should be in range 0..4096");
    }

    let height = match height {
        Variant::Int(height) => *height,
        _ => bail!("snow: invalid height type"),
    };

    if !(0..=4096).contains(&height) {
        bail!("snow: height should be in range 0..4096");
    }

    let arg3 = match arg3 {
        Variant::Int(arg3) => *arg3,
        _ => bail!("snow: invalid arg3 type"),
    };

    if !(0..=4095).contains(&arg3) {
        bail!("snow: arg3 should be in range 0..4095");
    }

    let arg4 = match arg4 {
        Variant::Int(arg4) => *arg4,
        _ => bail!("snow: invalid arg4 type"),
    };

    if !(2..=64).contains(&arg4) {
        bail!("snow: arg4 should be in range 2..64");
    }

    let arg5 = match arg5 {
        Variant::Int(arg5) => *arg5,
        _ => bail!("snow: invalid speed type"),
    };

    if !(2..=64).contains(&arg5) {
        bail!("snow: arg5 should be in range 2..64");
    }

    let arg6 = match arg6 {
        Variant::Int(arg6) => *arg6,
        _ => bail!("snow: invalid arg6 type"),
    };

    if !(1..=16).contains(&arg6) {
        bail!("snow: arg6 should be in range 1..16");
    }

    let arg7 = match arg7 {
        Variant::Int(arg7) => *arg7,
        _ => bail!("snow: invalid arg7 type"),
    };

    if !(10..=10000).contains(&arg7) {
        bail!("snow: arg7 should be in range 10..10000");
    }

    let arg8 = match arg8 {
        Variant::Int(arg8) => *arg8,
        _ => bail!("snow: invalid arg8 type"),
    };

    if !(10..=10000).contains(&arg8) {
        bail!("snow: arg8 should be in range 10..10000");
    }

    let arg9 = match arg9 {
        Variant::Int(arg9) => *arg9,
        _ => bail!("snow: invalid arg9 type"),
    };

    if !(1..=1024).contains(&arg9) {
        bail!("snow: arg9 should be in range 1..1024");
    }

    let arg10 = match arg10 {
        Variant::Int(arg10) => *arg10,
        _ => bail!("snow: invalid arg10 type"),
    };

    if !(1..=1024).contains(&arg10) {
        bail!("snow: arg10 should be in range 1..1024");
    }

    let arg11 = match arg11 {
        Variant::Int(arg11) => *arg11,
        _ => bail!("snow: invalid arg11 type"),
    };

    if !(-4096..=4096).contains(&arg11) {
        bail!("snow: arg11 should be in range 0..4096");
    }

    let arg12 = match arg12 {
        Variant::Int(arg12) => *arg12,
        _ => bail!("snow: invalid arg12 type"),
    };

    if !(-4096..=4096).contains(&arg12) {
        bail!("snow: arg12 should be in range -4096..4096");
    }

    let arg13 = match arg13 {
        Variant::Int(arg13) => *arg13,
        _ => bail!("snow: invalid arg13 type"),
    };

    if !(0..=1024).contains(&arg13) {
        bail!("snow: arg13 should be in range 0..1024");
    }

    let arg14 = match arg14 {
        Variant::Int(arg14) => *arg14,
        _ => bail!("snow: invalid arg14 type"),
    };

    if !(0..=255).contains(&arg14) {
        bail!("snow: arg14 should be in range 0..255");
    }

    let arg15 = match arg15 {
        Variant::Int(arg15) => *arg15,
        _ => bail!("snow: invalid arg15 type"),
    };

    if !(0..=255).contains(&arg15) {
        bail!("snow: arg15 should be in range 0..255");
    }

    let arg16 = match arg16 {
        Variant::Int(arg16) => *arg16,
        _ => bail!("snow: invalid arg16 type"),
    };

    if !(10..=10000).contains(&arg16) {
        bail!("snow: arg16 should be in range 10..10000");
    }

    let arg17 = match arg17 {
        Variant::Int(arg17) => *arg17,
        _ => bail!("snow: invalid arg17 type"),
    };

    if !(0..=255).contains(&arg17) {
        bail!("snow: arg17 should be in range 0..255");
    }

    let screen_width = game_data.get_width();
    let screen_height = game_data.get_height();

    game_data.motion_manager.set_snow_motion(
        id as u32, 
        width,
        height,
        arg3 as i32,
        arg4 as i32,
        arg5 as i32,
        arg6 as i32,
        arg7 as i32,
        arg8 as i32,
        arg9 as i32,
        arg10 as i32,
        arg11 as i32,
        arg12 as i32,
        arg13 as i32,
        arg14 as i32,
        arg15 as i32,
        arg16 as i32,
        arg17 as i32,
        screen_width,
        screen_height,
    );

    Ok(Variant::Nil)
}


pub fn snow_start(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("snow_start: invalid id type"),
    };

    if !(0..=1).contains(&id) {
        bail!("snow_start: id should be in range 0..1");
    }

    game_data.motion_manager.start_snow_motion(id as u32);

    Ok(Variant::Nil)
}

pub fn snow_stop(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("snow_stop: invalid id type"),
    };

    if !(0..=1).contains(&id) {
        bail!("snow_stop: id should be in range 0..1");
    }

    game_data.motion_manager.stop_snow_motion(id as u32);

    Ok(Variant::Nil)
}


pub struct LipAnim;
impl Syscaller for LipAnim {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        lip_anim(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
            get_var!(args, 3),
            get_var!(args, 4),
            get_var!(args, 5),
            get_var!(args, 6),
            get_var!(args, 7),
        )
    }
}

unsafe impl Send for LipAnim {}
unsafe impl Sync for LipAnim {}

pub struct LipSync;
impl Syscaller for LipSync {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        lip_sync(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for LipSync {}
unsafe impl Sync for LipSync {}

pub struct Dissolve;
impl Syscaller for Dissolve {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        dissolve(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
            get_var!(args, 3),
            get_var!(args, 4),
            get_var!(args, 5),
            get_var!(args, 6),
        )
    }
}

unsafe impl Send for Dissolve {}
unsafe impl Sync for Dissolve {}


pub struct Snow;
impl Syscaller for Snow {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        snow(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
            get_var!(args, 3),
            get_var!(args, 4),
            get_var!(args, 5),
            get_var!(args, 6),
            get_var!(args, 7),
            get_var!(args, 8),
            get_var!(args, 9),
            get_var!(args, 10),
            get_var!(args, 11),
            get_var!(args, 12),
            get_var!(args, 13),
            get_var!(args, 14),
            get_var!(args, 15),
            get_var!(args, 16),
            get_var!(args, 17),
        )
    }
}


unsafe impl Send for Snow {}
unsafe impl Sync for Snow {}

pub struct SnowStart;
impl Syscaller for SnowStart {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        snow_start(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for SnowStart {}
unsafe impl Sync for SnowStart {}


pub struct SnowStop;
impl Syscaller for SnowStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        snow_stop(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for SnowStop {}
unsafe impl Sync for SnowStop {}
