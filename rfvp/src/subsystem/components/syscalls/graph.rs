use anyhow::{Result};

use crate::script::Variant;
use crate::subsystem::resources::prim::PrimType;
use crate::subsystem::world::GameData;

use super::Syscaller;

pub fn prim_exit_group(_game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_exit_group: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=4095).contains(&id) {
        log::error!("prim_exit_group: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    Ok(Variant::Nil)
}

pub fn prim_group_in(game_data: &mut GameData, id: &Variant, id2: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_group_in: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_group_in: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let id2 = match id2.as_int() {
        Some(id2) => id2,
        None => {
            log::error!("prim_group_in: invalid id2 : {:?}", id2);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=4095).contains(&id2) {
        log::error!("prim_group_in: invalid id2 : {}", id2);
        return Ok(Variant::Nil);
    }

    game_data
        .motion_manager
        .prim_manager
        .set_prim_group_in(id2, id);

    Ok(Variant::Nil)
}

pub fn prim_group_move(game_data: &mut GameData, id: &Variant, id2: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_group_move: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_group_move: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let id2 = match id2.as_int() {
        Some(id2) => id2,
        None => {
            log::error!("prim_group_move: invalid id2 : {:?}", id2);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id2) {
        log::error!("prim_group_move: invalid id2 : {}", id2);
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.prim_manager.prim_move(id2, id);

    Ok(Variant::Nil)
}

pub fn prim_group_out(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_group_out: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_group_out: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.prim_manager.unlink_prim(id as i16);

    Ok(Variant::Nil)
}

/// reset the primitive
pub fn prim_set_null(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_null_parent: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_null_parent: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    game_data
        .motion_manager
        .prim_manager
        .prim_init_with_type(id as i16, PrimType::PrimTypeNone);

    Ok(Variant::Nil)
}

/// set the primitive's alpha value
pub fn prim_set_alpha(game_data: &mut GameData, id: &Variant, alpha: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_alpha: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_alpha: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let alpha = match alpha.as_int() {
        Some(alpha) => alpha,
        None => {
            log::error!("prim_set_alpha: invalid alpha : {:?}", alpha);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=255).contains(&alpha) {
        log::error!("prim_set_alpha: invalid alpha : {}", alpha);
        return Ok(Variant::Nil);
    }

    game_data
        .motion_manager
        .prim_manager
        .prim_set_alpha(id, alpha);

    Ok(Variant::Nil)
}

pub fn prim_set_blend(game_data: &mut GameData, id: &Variant, blend: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_blend: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=4095).contains(&id) {
        log::error!("prim_set_blend: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let blend = match blend.as_int() {
        Some(blend) => blend,
        None => {
            log::error!("prim_set_blend: invalid blend : {:?}", blend);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=1).contains(&blend) {
        log::error!("prim_set_blend: invalid blend : {}", blend);
        return Ok(Variant::Nil);
    }

    game_data
        .motion_manager
        .prim_manager
        .prim_set_blend(id, blend);

    Ok(Variant::Nil)
}

pub fn prim_set_draw(game_data: &mut GameData, id: &Variant, draw: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_draw: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_draw: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let draw = match draw.as_int() {
        Some(draw) => draw,
        None => {
            log::error!("prim_set_draw: invalid draw : {:?}", draw);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=1).contains(&draw) {
        log::error!("prim_set_draw: invalid draw : {}", draw);
        return Ok(Variant::Nil);
    }

    game_data
        .motion_manager
        .prim_manager
        .prim_set_draw(id, draw);

    Ok(Variant::Nil)
}

// set the primitive's archor point
pub fn prim_set_op(
    game_data: &mut GameData,
    id: &Variant,
    opx: &Variant,
    opy: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_op: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_op: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let opx = match opx.as_int() {
        Some(op) => op,
        None => {
            log::error!("prim_set_op: invalid op : {:?}", opx);
            return Ok(Variant::Nil);
        },
    };

    let opy = match opy.as_int() {
        Some(op) => op,
        None => {
            log::error!("prim_set_op: invalid op : {:?}", opy);
            return Ok(Variant::Nil);
        },
    };

    game_data
        .motion_manager
        .prim_manager
        .prim_set_op(id, opx, opy);

    Ok(Variant::Nil)
}

/// set the primitive's rotation and scale, and the scale value is the same in x and y
pub fn prim_set_rs(
    game_data: &mut GameData,
    id: &Variant,
    rotation: &Variant,
    scale: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_rs: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_rs: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let rotation = match rotation.as_int() {
        Some(r) => {
            let r2 = r % 3600;
            if r2 < 0 {
                r2 + 3600
            } else {
                r2
            }
        }
        None => {
            log::error!("prim_set_rs: invalid rs : {:?}", rotation);
            return Ok(Variant::Nil);
        },
    };

    let scale = match scale.as_int() {
        Some(s) => s,
        None => {
            log::error!("prim_set_rs: invalid rs : {:?}", scale);
            return Ok(Variant::Nil);
        },
    };

    let scale = if !(0..=10000).contains(&scale) {
        100 // default value
    } else {
        scale
    };

    game_data
        .motion_manager
        .prim_manager
        .prim_set_rotation(id, rotation);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_scale(id, scale, scale);
    game_data
        .motion_manager
        .prim_manager
        .prim_add_attr(id, 0x40);

    Ok(Variant::Nil)
}

/// set the primitive's rotation and scale, and the scale value is different in x and y
pub fn prim_set_rs2(
    game_data: &mut GameData,
    id: &Variant,
    rotation: &Variant,
    scale_x: &Variant,
    scale_y: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_rs2: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_rs2: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let rotation = match rotation.as_int() {
        Some(r) => {
            let r2 = r % 3600;
            if r2 < 0 {
                r2 + 3600
            } else {
                r2
            }
        }
        None => {
            log::error!("prim_set_rs2: invalid rs : {:?}", rotation);
            return Ok(Variant::Nil);
        },
    };

    let scale_x = match scale_x.as_int() {
        Some(s) => s,
        None => {
            log::error!("prim_set_rs2: invalid rs : {:?}", scale_x);
            return Ok(Variant::Nil);
        },
    };

    let scale_y = match scale_y.as_int() {
        Some(s) => s,
        None => {
            log::error!("prim_set_rs2: invalid rs : {:?}", scale_y);
            return Ok(Variant::Nil);
        },
    };

    let scale_x = if !(0..=10000).contains(&scale_x) {
        100 // default value
    } else {
        scale_x
    };

    let scale_y = if !(0..=10000).contains(&scale_y) {
        100 // default value
    } else {
        scale_y
    };

    game_data
        .motion_manager
        .prim_manager
        .prim_set_rotation(id, rotation);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_scale(id, scale_x, scale_y);
    game_data
        .motion_manager
        .prim_manager
        .prim_add_attr(id, 0x40);

    Ok(Variant::Nil)
}

pub fn prim_set_snow(
    game_data: &mut GameData,
    id: &Variant,
    mode: &Variant,
    x: &Variant,
    y: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_snow: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_snow: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let mode = match mode.as_int() {
        Some(mode) => mode,
        None => {
            log::error!("prim_set_snow: invalid mode : {:?}", mode);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=1).contains(&mode) {
        log::error!("prim_set_snow: invalid mode : {}", mode);
        return Ok(Variant::Nil);
    }

    let x = x.as_int().unwrap_or(0);
    let y = y.as_int().unwrap_or(0);

    game_data
        .motion_manager
        .prim_manager
        .prim_init_with_type(id as i16, PrimType::PrimTypeSnow);
    game_data.motion_manager.prim_manager.prim_set_op(id, 0, 0);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_alpha(id, 255i32);
    game_data.motion_manager.prim_manager.prim_set_blend(id, 0);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_rotation(id, 0);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_scale(id, 1000, 1000);
    game_data.motion_manager.prim_manager.prim_set_uv(id, 0, 0);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_size(id, 0, 0);
    game_data.motion_manager.prim_manager.prim_set_pos(id, x, y);
    game_data.motion_manager.prim_manager.prim_set_z(id, 1000);
    game_data.motion_manager.prim_manager.prim_set_attr(id, 0);

    Ok(Variant::Nil)
}

pub fn prim_set_sprt(
    game_data: &mut GameData,
    id: &Variant,
    src_id: &Variant,
    x: &Variant,
    y: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_snow: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_snow: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let src_id = match src_id.as_int() {
        Some(src_id) => src_id,
        None => {
            log::error!("prim_set_snow: invalid src_id : {:?}", src_id);
            return Ok(Variant::Nil);
        },
    };

    if !(-2..=4095).contains(&src_id) {
        log::error!("prim_set_snow: invalid src_id : {}", src_id);
        return Ok(Variant::Nil);
    }

    let x = x.as_int().unwrap_or(0);
    let y = y.as_int().unwrap_or(0);

    game_data
        .motion_manager
        .prim_manager
        .prim_init_with_type(id as i16, PrimType::PrimTypeSprt);
    game_data.motion_manager.prim_manager.prim_set_op(id, 0, 0);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_alpha(id, 255i32);
    game_data.motion_manager.prim_manager.prim_set_blend(id, 0);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_rotation(id, 0);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_scale(id, 1000, 1000);
    game_data.motion_manager.prim_manager.prim_set_uv(id, 0, 0);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_size(id, 0, 0);
    game_data.motion_manager.prim_manager.prim_set_pos(id, x, y);
    game_data.motion_manager.prim_manager.prim_set_z(id, 1000);
    game_data.motion_manager.prim_manager.prim_set_texture_id(id, 0);
    game_data.motion_manager.prim_manager.prim_set_attr(id, 0);

    Ok(Variant::Nil)
}

pub fn prim_set_text(
    game_data: &mut GameData,
    id: &Variant,
    text_id: &Variant,
    x: &Variant,
    y: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_text: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_text: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let text_id = match text_id.as_int() {
        Some(text_id) => text_id,
        None => {
            log::error!("prim_set_text: invalid text_id : {:?}", text_id);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=31).contains(&text_id) {
        log::error!("prim_set_text: invalid text_id : {}", text_id);
        return Ok(Variant::Nil);
    }

    let x = x.as_int().unwrap_or(0);
    let y = y.as_int().unwrap_or(0);

    game_data
        .motion_manager
        .prim_manager
        .prim_init_with_type(id as i16, PrimType::PrimTypeText);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_alpha(id, 255i32);
    game_data.motion_manager.prim_manager.prim_set_blend(id, 0);
    game_data.motion_manager.prim_manager.prim_set_pos(id, x, y);
    game_data
        .motion_manager
        .prim_manager
        .prim_remove_attr(id, 0xFE);

    Ok(Variant::Nil)
}

pub fn prim_set_tile(
    game_data: &mut GameData,
    id: &Variant,
    tile_id: &Variant,
    x: &Variant,
    y: &Variant,
    w: &Variant,
    h: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_tile: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_tile: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let tile_id = match tile_id.as_int() {
        Some(tile_id) => tile_id,
        None => {
            log::error!("prim_set_tile: invalid tile_id : {:?}", tile_id);
            return Ok(Variant::Nil);
        },
    };

    if !(0..=31).contains(&tile_id) {
        log::error!("prim_set_tile: invalid tile_id : {}", tile_id);
        return Ok(Variant::Nil);
    }

    let x = x.as_int().unwrap_or(0);
    let y = y.as_int().unwrap_or(0);
    let w = w.as_int().unwrap_or(0);
    let h = h.as_int().unwrap_or(0);

    game_data
        .motion_manager
        .prim_manager
        .prim_init_with_type(id as i16, PrimType::PrimTypeTile);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_alpha(id, 255i32);
    game_data.motion_manager.prim_manager.prim_set_blend(id, 0);
    game_data.motion_manager.prim_manager.prim_set_pos(id, x, y);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_size(id, w, h);
    game_data
        .motion_manager
        .prim_manager
        .prim_set_tile(id, tile_id);

    Ok(Variant::Nil)
}

pub fn prim_set_uv(
    game_data: &mut GameData,
    id: &Variant,
    u: &Variant,
    v: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_uv: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_uv: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let u = match u.as_int() {
        Some(u) => u,
        None => {
            log::error!("prim_set_uv: invalid u : {:?}", u);
            return Ok(Variant::Nil);
        },
    };

    let v = match v.as_int() {
        Some(v) => v,
        None => {
            log::error!("prim_set_uv: invalid v : {:?}", v);
            return Ok(Variant::Nil);
        },
    };

    game_data.motion_manager.prim_manager.prim_set_uv(id, u, v);
    game_data.motion_manager.prim_manager.prim_add_attr(id, 1);
    game_data
        .motion_manager
        .prim_manager
        .prim_add_attr(id, 0x40);

    Ok(Variant::Nil)
}

pub fn prim_set_xy(
    game_data: &mut GameData,
    id: &Variant,
    x: &Variant,
    y: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_xy: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_xy: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let x = match x.as_int() {
        Some(x) => x,
        None => {
            log::error!("prim_set_xy: invalid x : {:?}", x);
            return Ok(Variant::Nil);
        },
    };

    let y = match y.as_int() {
        Some(y) => y,
        None => {
            log::error!("prim_set_xy: invalid y : {:?}", y);
            return Ok(Variant::Nil);
        },
    };

    game_data.motion_manager.prim_manager.prim_set_pos(id, x, y);
    game_data
        .motion_manager
        .prim_manager
        .prim_add_attr(id, 0x40);

    Ok(Variant::Nil)
}

pub fn prim_set_wh(
    game_data: &mut GameData,
    id: &Variant,
    w: &Variant,
    h: &Variant,
) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_wh: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_wh: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let w = match w.as_int() {
        Some(w) => w,
        None => {
            log::error!("prim_set_wh: invalid w : {:?}", w);
            return Ok(Variant::Nil);
        },
    };

    let h = match h.as_int() {
        Some(h) => h,
        None => {
            log::error!("prim_set_wh: invalid h : {:?}", h);
            return Ok(Variant::Nil);
        },
    };

    game_data
        .motion_manager
        .prim_manager
        .prim_set_size(id, w, h);
    game_data
        .motion_manager
        .prim_manager
        .prim_add_attr(id, 0x40);
    game_data.motion_manager.prim_manager.prim_add_attr(id, 1);

    Ok(Variant::Nil)
}

pub fn prim_set_z(game_data: &mut GameData, id: &Variant, z: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_set_z: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_set_z: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let z = match z.as_int() {
        Some(z) => {
            if z < 100 {
                100
            } else if z > 10000 {
                10000
            } else {
                z
            }
        }
        None => {
            log::error!("prim_set_z: invalid z : {:?}", z);
            return Ok(Variant::Nil);
        },
    };

    game_data.motion_manager.prim_manager.prim_set_z(id, z);
    match game_data.motion_manager.prim_manager.prim_get_type(id) {
        PrimType::PrimTypeNone => {}
        PrimType::PrimTypeGroup | PrimType::PrimTypeTile => {
            game_data
                .motion_manager
                .prim_manager
                .prim_add_attr(id, 0x40);
            game_data.motion_manager.prim_manager.prim_add_attr(id, 4);
        }
        _ => {
            game_data
                .motion_manager
                .prim_manager
                .prim_add_attr(id, 0x40);
            game_data
                .motion_manager
                .prim_manager
                .prim_remove_attr(id, 0xFB);
        }
    };

    Ok(Variant::Nil)
}

pub fn prim_hit(game_data: &mut GameData, id: &Variant, flag: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("prim_hit: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(1..=4095).contains(&id) {
        log::error!("prim_hit: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let flag = flag.canbe_true();

    game_data
        .motion_manager
        .prim_hit(
            id, 
            flag, 
            game_data.inputs_manager.get_cursor_in(), 
            game_data.inputs_manager.get_cursor_x(),
            game_data.inputs_manager.get_cursor_y(),
        );

    Ok(Variant::Nil)
}


pub fn graph_load(game_data: &mut GameData, id: &Variant, path: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("graph_load: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(0..4096).contains(&id) {
        log::error!("graph_load: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    match path {
        Variant::String(path) | Variant::ConstString(path, _) => {
            let buff = game_data.vfs_load_file(path)?;
            game_data
                .motion_manager
                .load_graph(id as u16, path, buff)?;
            game_data
                .motion_manager
                .refresh_prims(id as u16);
        }
        Variant::Nil => {
            game_data
                .motion_manager
                .unload_graph(id as u16);
        }
        _ => {
            log::error!("graph_load: invalid path : {:?}", path);
            return Ok(Variant::Nil);
        },
    }

    Ok(Variant::Nil)
}


pub fn graph_rgb(game_data: &mut GameData, id: &Variant, r: &Variant, g: &Variant, b: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => {
            log::error!("graph_rgb: invalid id : {:?}", id);
            return Ok(Variant::Nil);
        },
    };

    if !(0..4096).contains(&id) {
        log::error!("graph_rgb: invalid id : {}", id);
        return Ok(Variant::Nil);
    }

    let r = match r.as_int() {
        Some(r) => {
            if !(0..=200).contains(&r) {
                100
            } else {
                r
            }
        },
        None => {
            log::error!("graph_rgb: invalid r : {:?}", r);
            return Ok(Variant::Nil);
        },
    };

    let g = match g.as_int() {
        Some(g) => {
            if !(0..=200).contains(&g) {
                100
            } else {
                g
            }
        },
        None => {
            log::error!("graph_rgb: invalid g : {:?}", g);
            return Ok(Variant::Nil);
        },
    };

    let b = match b.as_int() {
        Some(b) => {
            if !(0..=200).contains(&b) {
                100
            } else {
                b
            }
        },
        None => {
            log::error!("graph_rgb: invalid b : {:?}", b);
            return Ok(Variant::Nil);
        },
    };


    game_data
        .motion_manager
        .graph_color_tone(id as u16, r, g, b);

    Ok(Variant::Nil)
}


pub fn gaiji_load(game_data: &mut GameData, code: &Variant, size: &Variant, fname: &Variant) -> Result<Variant> {
    let fname = match fname.as_string() {
        Some(fname) => fname,
        None => {
            log::error!("gaiji_load: invalid fname : {:?}", fname);
            return Ok(Variant::Nil);
        },
    };

    let size = match size.as_int() {
        Some(size) => size,
        None => {
            log::error!("gaiji_load: invalid size : {:?}", size);
            return Ok(Variant::Nil);
        },
    };

    let code = match code.as_string() {
        Some(code) => code,
        None => {
            log::error!("gaiji_load: invalid code : {:?}", code);
            return Ok(Variant::Nil);
        },
    };

    if code.is_empty() {
        log::error!("gaiji_load: empty code : {:?}", code);
        return Ok(Variant::Nil);
    }

    let code = code.chars().collect::<Vec<_>>().first().unwrap().to_owned();
    let buff = game_data.vfs_load_file(fname)?;

    game_data
        .motion_manager
        .set_gaiji(code, size as u8, fname, buff)?;

    Ok(Variant::Nil)
}

pub struct PrimExitGroup;
impl Syscaller for PrimExitGroup {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_exit_group(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for PrimExitGroup {}
unsafe impl Sync for PrimExitGroup {}

pub struct PrimGroupIn;
impl Syscaller for PrimGroupIn {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_group_in(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for PrimGroupIn {}
unsafe impl Sync for PrimGroupIn {}

pub struct PrimGroupMove;
impl Syscaller for PrimGroupMove {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_group_move(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for PrimGroupMove {}
unsafe impl Sync for PrimGroupMove {}

pub struct PrimGroupOut;
impl Syscaller for PrimGroupOut {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_group_out(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for PrimGroupOut {}
unsafe impl Sync for PrimGroupOut {}

pub struct PrimSetNull;
impl Syscaller for PrimSetNull {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_null(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for PrimSetNull {}
unsafe impl Sync for PrimSetNull {}

pub struct PrimSetAlpha;
impl Syscaller for PrimSetAlpha {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_alpha(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for PrimSetAlpha {}
unsafe impl Sync for PrimSetAlpha {}

pub struct PrimSetBlend;
impl Syscaller for PrimSetBlend {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_blend(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for PrimSetBlend {}
unsafe impl Sync for PrimSetBlend {}

pub struct PrimSetDraw;
impl Syscaller for PrimSetDraw {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_draw(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for PrimSetDraw {}
unsafe impl Sync for PrimSetDraw {}

pub struct PrimSetOp;
impl Syscaller for PrimSetOp {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_op(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for PrimSetOp {}
unsafe impl Sync for PrimSetOp {}

pub struct PrimSetRS;
impl Syscaller for PrimSetRS {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_rs(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for PrimSetRS {}
unsafe impl Sync for PrimSetRS {}

pub struct PrimSetRS2;
impl Syscaller for PrimSetRS2 {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_rs2(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
            super::get_var!(args, 3),
        )
    }
}

unsafe impl Send for PrimSetRS2 {}
unsafe impl Sync for PrimSetRS2 {}

pub struct PrimSetSnow;
impl Syscaller for PrimSetSnow {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_snow(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
            super::get_var!(args, 3),
        )
    }
}

unsafe impl Send for PrimSetSnow {}
unsafe impl Sync for PrimSetSnow {}

pub struct PrimSetSprt;
impl Syscaller for PrimSetSprt {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_sprt(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
            super::get_var!(args, 3),
        )
    }
}

unsafe impl Send for PrimSetSprt {}
unsafe impl Sync for PrimSetSprt {}

pub struct PrimSetText;
impl Syscaller for PrimSetText {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_text(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
            super::get_var!(args, 3),
        )
    }
}

unsafe impl Send for PrimSetText {}
unsafe impl Sync for PrimSetText {}

pub struct PrimSetTile;
impl Syscaller for PrimSetTile {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_tile(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
            super::get_var!(args, 3),
            super::get_var!(args, 4),
            super::get_var!(args, 5),
        )
    }
}

unsafe impl Send for PrimSetTile {}
unsafe impl Sync for PrimSetTile {}

pub struct PrimSetUV;
impl Syscaller for PrimSetUV {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_uv(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for PrimSetUV {}
unsafe impl Sync for PrimSetUV {}

pub struct PrimSetXY;
impl Syscaller for PrimSetXY {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_xy(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for PrimSetXY {}
unsafe impl Sync for PrimSetXY {}

pub struct PrimSetWH;
impl Syscaller for PrimSetWH {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_wh(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for PrimSetWH {}
unsafe impl Sync for PrimSetWH {}

pub struct PrimSetZ;
impl Syscaller for PrimSetZ {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_z(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for PrimSetZ {}
unsafe impl Sync for PrimSetZ {}


pub struct PrimHit;
impl Syscaller for PrimHit {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_hit(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for PrimHit {}
unsafe impl Sync for PrimHit {}


pub struct GraphLoad;
impl Syscaller for GraphLoad {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        graph_load(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for GraphLoad {}
unsafe impl Sync for GraphLoad {}


pub struct GraphRGB;
impl Syscaller for GraphRGB {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        graph_rgb(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
            super::get_var!(args, 3),
        )
    }
}

unsafe impl Send for GraphRGB {}
unsafe impl Sync for GraphRGB {}


pub struct GaijiLoad;
impl Syscaller for GaijiLoad {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        gaiji_load(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for GaijiLoad {}
unsafe impl Sync for GaijiLoad {}

