use anyhow::{Result};

use crate::script::Variant;
use crate::subsystem::resources::prim::PrimType;
use crate::subsystem::world::GameData;

use super::Syscaller;

pub fn prim_exit_group(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let Some(id) = id.as_int() else {
        log::error!("prim_exit_group: invalid id : {:?}", id);
        return Ok(Variant::Nil);
    };

    crate::trace::prim_evt(format_args!("prim_exit_group: id={}", id));

    // Match original engine: accept only 0..4095, ignore others.
    if !(0..=4095).contains(&id) {
        // Optional: warn instead of error to match "silently ignore"
        log::warn!("prim_exit_group: ignored out-of-range id={}", id);
        return Ok(Variant::Nil);
    }

    // Store scene root prim index.
    game_data.set_prim_root(id as i16);

    // Keep renderer traversal root in sync.
    game_data
        .motion_manager
        .prim_manager
        .set_custom_root_prim_id(id as u16);

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

    // In real scripts, rotation/scale are often passed as Nil to indicate "keep current".
    // Treat Nil (and other unexpected types) as "no change" instead of reporting an error.
    let (cur_rot, cur_fx) = {
        let p = game_data
            .motion_manager
            .prim_manager
            .get_prim_immutable(id as i16);
        (p.get_rotation() as i32, p.get_factor_x() as i32)
    };

    let rotation = match rotation.as_int() {
        Some(r) => {
            let r2 = r % 3600;
            if r2 < 0 { r2 + 3600 } else { r2 }
        }
        None => cur_rot,
    };

    let scale = match scale.as_int() {
        Some(s) => s,
        None => cur_fx,
    };

    let scale = if !(0..=10000).contains(&scale) { 1000 } else { scale };

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

    // Nil indicates "keep current".
    let (cur_rot, cur_fx, cur_fy) = {
        let p = game_data
            .motion_manager
            .prim_manager
            .get_prim_immutable(id as i16);
        (
            p.get_rotation() as i32,
            p.get_factor_x() as i32,
            p.get_factor_y() as i32,
        )
    };

    let rotation = match rotation.as_int() {
        Some(r) => {
            let r2 = r % 3600;
            if r2 < 0 { r2 + 3600 } else { r2 }
        }
        None => cur_rot,
    };

    let scale_x = match scale_x.as_int() {
        Some(s) => s,
        None => cur_fx,
    };

    let scale_y = match scale_y.as_int() {
        Some(s) => s,
        None => cur_fy,
    };

    let scale_x = if !(0..=10000).contains(&scale_x) { 1000 } else { scale_x };
    let scale_y = if !(0..=10000).contains(&scale_y) { 1000 } else { scale_y };

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
    // Bind sprite source texture id.
    game_data
        .motion_manager
        .prim_manager
        .prim_set_texture_id(id, src_id);
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
    game_data.motion_manager.prim_manager.prim_set_text(id, text_id);
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

    let x = if !x.is_nil() {
        match x.as_int() {
            Some(x) => x,
            None => {
                log::error!("prim_set_xy: invalid x : {:?}", x);
                return Ok(Variant::Nil);
            },
        }
    } else {
        game_data.motion_manager.prim_manager.get_prim(id as i16).get_x().into()
    };

    let y = if !y.is_nil() {
        match y.as_int() {
            Some(y) => y,
            None => {
                log::error!("prim_set_xy: invalid y : {:?}", y);
                return Ok(Variant::Nil);
            },
        }
    } else {
        game_data.motion_manager.prim_manager.get_prim(id as i16).get_y().into()
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

    let w = if !w.is_nil() {
        match w.as_int() {
            Some(w) => w,
            None => {
                log::error!("prim_set_wh: invalid w : {:?}", w);
                return Ok(Variant::Nil);
            },
        }
    } else {
        game_data.motion_manager.prim_manager.get_prim(id as i16).get_w().into()
    };

    let h = if !h.is_nil() {
        match h.as_int() {
            Some(h) => h,
            None => {
                log::error!("prim_set_wh: invalid h : {:?}", h);
                return Ok(Variant::Nil);
            },
        }
    } else {
        game_data.motion_manager.prim_manager.get_prim(id as i16).get_h().into()
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

    // Reverse-engineered semantics (do not reorder prim tree here):
    // - If z is Int: clamp to [100, 10000], set prim.z, and enable attr bit 0x04.
    // - If z is Float: do NOT change prim.z, but still enable attr bit 0x04.
    // - Otherwise: disable attr bit 0x04.
    // In all cases, mark dirty via attr bit 0x40.

    if let Some(z_i) = z.as_int() {
        let z_i = z_i.clamp(100, 10000);
        game_data.motion_manager.prim_manager.prim_set_z(id, z_i);
        game_data.motion_manager.prim_manager.prim_add_attr(id, 0x04);
        game_data.motion_manager.prim_manager.prim_add_attr(id, 0x40);
        return Ok(Variant::Nil);
    }

    if z.as_float().is_some() {
        game_data.motion_manager.prim_manager.prim_add_attr(id, 0x04);
        game_data.motion_manager.prim_manager.prim_add_attr(id, 0x40);
        return Ok(Variant::Nil);
    }

    // nil/other types: clear 0x04, keep other bits as-is.
    {
        let attr = {
            let p = game_data.motion_manager.prim_manager.get_prim_immutable(id as i16);
            p.get_attr()
        };
        game_data
            .motion_manager
            .prim_manager
            .prim_set_attr(id, (attr & !0x04) as i32);
        game_data.motion_manager.prim_manager.prim_add_attr(id, 0x40);
    }

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


///
/// Set the root primitive index
/// The engine will begin rendering from this primitive.
/// Arg1: the primitive, which should not larger that 0x1000
/// 
pub struct PrimExitGroup;
impl Syscaller for PrimExitGroup {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_exit_group(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for PrimExitGroup {}
unsafe impl Sync for PrimExitGroup {}

/// Add a primitive as a child of another primitive (group insertion).
///
/// Arg1: child primitive index (1–4095)
/// Arg2: parent primitive index (0–4095)
///
/// Behavior:
/// - Initializes the parent as a group (type 1) if needed.
/// - Removes the child from any previous parent/sibling links.
/// - Appends the child to the end of the parent’s child list.
/// - Marks the child as part of a group (m_Attribute |= 0x40).
///
/// Example usage (script):
///   PrimGroupIn(10, 5)  // Make primitive 10 a child of primitive 5
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

/// Move a primitive in the scene hierarchy relative to another primitive.
///
/// Arg1: reference primitive index (`idx2`)
///       - The primitive after/below which the target will be inserted.
/// Arg2: target primitive index (`idx`)
///       - The primitive that will be moved.
///
/// Behavior:
/// - Detaches the target primitive from its current parent and sibling chain.
/// - Attaches it under the same parent as `idx2`.
/// - Updates sibling links so that `idx` follows `idx2`:
///     - If `idx2` has no next sibling, `idx` becomes the new last child of the parent.
///     - Otherwise, `idx` is inserted between `idx2` and `idx2`'s next sibling.
/// - Sets the `0x40` flag on the target primitive to indicate it has been moved.
///
/// Hierarchy diagram (before and after moving `idx` relative to `idx2`):
///
/// Before:
/// Parent
/// ├── idx2
/// └── idx2_next_sibling
///
/// After:
/// Parent
/// ├── idx2
/// ├── idx      <- moved here
/// └── idx2_next_sibling
///
/// Example usage (script):
/// ```text
/// PrimGroupMove(5, 10)  // Move primitive 10 to follow primitive 5 under the same parent
/// ```
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


/// Remove a primitive from the scene hierarchy.
///
/// Arg1: primitive index (1–4095)
///
/// Behavior:
/// - Detaches the primitive from its parent and siblings using `unlink_prim`.
/// - The primitive itself remains allocated and can be reinserted.
///
/// Example usage (script):
///   PrimGroupOut(10)  // Removes primitive 10 from the hierarchy
pub struct PrimGroupOut;
impl Syscaller for PrimGroupOut {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_group_out(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for PrimGroupOut {}
unsafe impl Sync for PrimGroupOut {}

///
/// Reset (clear) the corresponding primitive
///
/// Arg1: primitive index
///
/// This syscall reinitializes the specified primitive to a "null" state.
/// The primitive is detached from its parent/sibling chain, its attributes
/// are cleared, and it is ready to be reused.  
/// 
/// Typical usage: free an existing primitive before creating a new one
/// in the same slot.
///
pub struct PrimSetNull;
impl Syscaller for PrimSetNull {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_null(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for PrimSetNull {}
unsafe impl Sync for PrimSetNull {}

///
/// Set the alpha (transparency) value of the corresponding primitive
///
/// Arg1: primitive index  
/// Arg2: alpha value (0–255)
///
/// This syscall changes the transparency of a primitive.  
/// - `0` means fully transparent  
/// - `255` means fully opaque  
///
/// The alpha value is usually used in blending during rendering.  
/// Intermediate values create semi-transparent effects.
///
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


///
/// Set the blending mode flag of the corresponding primitive
///
/// Arg1: primitive index
/// Arg2: blend flag
///
/// - blend = 0: use inverse source color blending (D3DBLEND_INVSRCCOLOR)
/// - blend = 1: use additive blending (D3DBLEND_ONE)
///
/// This syscall modifies how the primitive's color is combined with the
/// existing framebuffer. Combined with PrimSetAlpha, it controls transparency
/// and visual blending effects.
///
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

///
/// Set the draw flag of the corresponding primitive
/// Arg1: primitive index
/// Arg2: draw flag
/// 
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


///
/// Set the operation (position) of the corresponding primitive
///
/// Arg1: primitive index
/// Arg2: x coordinate (local, relative to parent) — signed integer (pixels)
/// Arg3: y coordinate (local, relative to parent) — signed integer (pixels)
///
/// This syscall sets the primitive's local position. The engine stores prim
/// coordinates relative to the parent (see drawing: parent's x/y are added
/// before drawing children). Changing position should mark the prim as
/// "dirty" so the renderer / layout logic can update any cached bounds or
/// vertex data.
///
/// Example usage (script):
///   PrimSetOp(42, 100, 200)  // set prim 42 to local position (100, 200)
///
pub struct PrimSetOP;
impl Syscaller for PrimSetOP {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        prim_set_op(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for PrimSetOP {}
unsafe impl Sync for PrimSetOP {}


///
/// Set the rotation and scale factor of the corresponding primitive
///
/// Arg1: primitive index
/// Arg2: rotation value (0–3600, representing 0–360 degrees; optional)
/// Arg3: scale factor (100 = 100%, range 100–10000; optional)
///
/// This syscall modifies the primitive's transformation parameters:
/// - Rotation: clockwise, in tenths of a degree. Values wrap around 3600.
/// - Scale: uniform scaling on X and Y axes, clamped to [100, 10000].
///
/// The primitive is marked as "dirty" (m_Attribute |= 0x40) so that the
/// renderer will recalculate its matrix before drawing.
///
/// Example usage (script):
///   PrimSetRS(42, 900, 150)  // prim 42 rotated 90°, scaled 150%
///   PrimSetRS(42, 1800)      // prim 42 rotated 180°, scale unchanged
///
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

///
/// Set the rotation and independent scale factors of the corresponding primitive
///
/// Arg1: primitive index
/// Arg2: rotation value (0–3600, representing 0–360 degrees; optional)
/// Arg3: scale factor X (100 = 100%, range 100–10000; optional)
/// Arg4: scale factor Y (100 = 100%, range 100–10000; optional)
///
/// This syscall modifies the primitive's transformation parameters:
/// - Rotation: clockwise, in tenths of a degree. Values wrap around 3600.
/// - Scale X / Y: independent scaling on X and Y axes, clamped to [100, 10000].
///
/// The primitive is marked as "dirty" (m_Attribute |= 0x40) so that the
/// renderer will recalculate its matrix before drawing.
///
/// Example usage (script):
///   PrimSetRS2(42, 900, 150, 200)  // prim 42 rotated 90°, scaled 150% X, 200% Y
///   PrimSetRS2(42, 1800)            // prim 42 rotated 180°, scale unchanged
///
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

///
/// Initialize a primitive as a sprite and bind it to a texture or special source.
///
/// Arg1: primitive index (1–4095)
/// Arg2: referred texture id
///       - >= 0: index into scene.textures[]
///       - -1: use built-in special texture (rendered by sub_42B740)
///       - -2: use dynamic MOV-to-texture binding
/// Arg3: initial X position (optional, default 0)
/// Arg4: initial Y position (optional, default 0)
///
/// The primitive is initialized with:
/// - Type = Sprite (4)
/// - Position = (X, Y, Z=1000)
/// - Scale = (1000, 1000) [100%]
/// - Rotation = 0
/// - Alpha = -1 (opaque)
/// - Blend = 0 (normal)
///
/// Example usage (script):
///   PrimSetSprt(10, 5)       // Sprite prim 10, using texture 5
///   PrimSetSprt(11, -1, 100, 200)  // Sprite prim 11, special -1 texture at (100,200)
///
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
///
/// Initialize a primitive as a text placeholder bound to a text object.
///
/// Arg1: primitive index (1–4095)
/// Arg2: text index (managed by the text system)
/// Arg3: initial X position
/// Arg4: initial Y position
///
/// The primitive is initialized with:
/// - Type = Text
/// - Position = (X, Y, Z=1000)
/// - Scale = (1000, 1000) [100%]
/// - Rotation = 0
/// - Alpha = -1 (opaque)
/// - Blend = 0 (normal)
///
/// Notes:
/// - The actual text content is managed separately via SysCallText* APIs,
///   using the given text_index.
/// - Prim only controls placement, visibility, alpha, blend, etc.
///
/// Example usage:
///   PrimSetText(20, 5, 100, 200);   // prim 20 displays text object #5 at (100,200)
///
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


///
/// Initialize a primitive as a solid-color tile
///
/// Arg1: primitive index (1–4095)
/// Arg2: tile index (reference ID for tile data)
/// Arg3: X position (screen space)
/// Arg4: Y position (screen space)
/// Arg5: Width
/// Arg6: Height
///
/// This syscall initializes the primitive as a "Tile" type:
/// - Associates the primitive with a tile resource (tile index)
/// - Places it at screen position (X, Y)
/// - Sets its dimensions to (Width, Height)
/// - Rotation defaults to 0
/// - Scale factors default to (1000, 1000) [100%]
/// - Alpha defaults to -1 (opaque)
/// - Blend defaults to 0 (normal blending)
///
/// Notes:
/// - Tile primitives are always filled rectangles, using the engine’s tile resource.
/// - Color/alpha adjustments should be applied separately using `PrimSetAlpha` or `PrimSetBlend`.
/// - Commonly used for UI panels, backgrounds, or simple colored blocks.
///
/// Example usage:
///   PrimSetTile(10, 3, 50, 50, 200, 100); // prim #10, uses tile #3, positioned at (50,50) with size 200×100
///
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


///
/// Set the UV sub-rectangle of a primitive
///
/// Arg1: primitive index (1–4095)  
/// Arg2: U coordinate (left/top pixel in the texture)  
/// Arg3: V coordinate (top pixel in the texture)  
///
/// Usage:
/// - Defines the texture sampling origin relative to the **loaded texture atlas**.
/// - Typically used together with `PrimSetUVWH` to specify width/height of the region.
/// - If not called, the entire texture is sampled by default.
///
/// Relationship with offsets:
/// - `offset_x` / `offset_y` (from HGraphBuff) define the **pivot point** of the sprite when drawn.  
/// - `U` / `V` define the **top-left corner of the texture region** to be sampled.  
/// - At render time, the engine subtracts `(offset_x, offset_y)` from `(m_U, m_V)` so that the sampled region aligns with the sprite pivot.
///
/// Example:
/// ```
/// // Sample a 32×32 icon starting at (64,128) inside a texture atlas
/// PrimSetUV(12, 64, 128);
/// ```
///
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


///
/// Set the width and height of a primitive's texture region
///
/// Arg1: primitive index (1–4095)  
/// Arg2: width (pixels)  
/// Arg3: height (pixels)  
///
/// Usage:
/// - Works together with `PrimSetUV`.  
/// - Defines the size (W,H) of the rectangular region sampled from the texture atlas.  
/// - If not called, defaults to the full texture size (graph_width/graph_height).  
///
/// Relationship with offsets:
/// - `PrimSetUV` sets the starting point (U,V) inside the texture.  
/// - `PrimSetWH` sets how large the region is.  
/// - Together, they form the complete sub-rectangle `(U,V,W,H)` to be drawn.  
///
/// Example:
/// ```
/// // Render a 64×64 sprite from texture atlas starting at (U=128, V=256)
/// PrimSetUV(10, 128, 256);
/// PrimSetWH(10, 64, 64);
/// ```
///
/// Notes:
/// - The actual on-screen size may still be affected by `PrimSetRS`/`PrimSetRS2` (scaling).  
/// - If `W` or `H` exceed the texture bounds, they will be clipped during rendering.
///
/// Texture Atlas
// +---------------------------------------------------+
// |                                                   |
// |             (U,V) Origin                          |
// |                +-------------------+              |
// |                |                   |              |
// |                |   Sub-Region      | <- from      |
// |                |   (W,H)           |    PrimSetWH |
// |                |                   |              |
// |                +-------------------+              |
// |                                                   |
// +---------------------------------------------------+
// Explanation:
// - `PrimSetUV(index, U, V)` sets the starting point (U,V) of the texture sample rectangle.
// - `PrimSetWH(index, W, H)` sets the width and height (W,H) of that rectangle.
// - The actual sampled region = rectangle starting at (U,V) with size (W,H).
// - This rectangle is then mapped to the sprite on screen.
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

///
/// Set the Z-depth of a sprite primitive.
///
/// Arg1: primitive index (1–4095)
/// Arg2: Z value (100–10000)
///       - 100 = closest to camera (appears on top)
///       - 10000 = farthest from camera (appears behind)
///
/// The primitive's Z value affects:
/// - Rendering order (near → far)
/// - Perspective scaling and projection
///
/// Example usage (script):
///   PrimSetZ(10, 500)    // Set sprite prim 10 to Z=500
///   PrimSetZ(11, 2000)   // Set sprite prim 11 farther back in scene
///
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

/// Check if a sprite primitive was clicked or "hit".
///
/// Arg1: primitive index (1–4095)
///
/// Returns:
/// - `nil` if the primitive was not hit
/// - `True` if the primitive was clicked
///
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

/// Load or unload a graphical texture into the engine.
///
/// Arg1: texture index (0..0x1000)
/// Arg2: file path to the texture
///       - `path`: load the texture from the specified path
///       - `nil`: unload the texture at this index
///
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

///
/// Adjust the color tone of a loaded texture.
///
/// Arg1: texture index
///       - If nil, the call is ignored
/// Arg2: red adjustment (optional, default 100)
///       - 0 = completely dark
///       - 100 = original color
///       - 101–200 = brightened color
/// Arg3: green adjustment (optional, default 100)
///       - Same scale as red
/// Arg4: blue adjustment (optional, default 100)
///       - Same scale as red
///
/// Notes:
/// - This function does not tile or replace the texture.
/// - It performs a color tone modification (brightness/darkening) per channel.
/// - Values are clamped: 0–200. 100 means 100% of original channel intensity.
/// - Values >100 brighten proportionally; values <100 darken proportionally.
/// - Internally, `apply_color_tone` modifies the pixel buffer directly.
///
/// Example usage (script):
///   GraphRGB(5)                // Texture 5, no change (100,100,100)
///   GraphRGB(5, 50, 100, 100)  // Texture 5, red darkened by 50%
///   GraphRGB(5, 150, 150, 150) // Texture 5, brighten all channels by ~50%
///
/// Color Tone Scale:
///
/// Darkening | Original | Brightening
///    0     ---|--- 100 ---|--- 200
/// Red/Green/Blue intensity
///
/// ASCII diagram (proportional):
///
///  200 ┤          #######
///  180 ┤         #######
///  160 ┤        #######
///  140 ┤       #######
///  120 ┤      #######
///  100 ┤###### Original
///   80 ┤######
///   60 ┤#####
///   40 ┤####
///   20 ┤###
///    0 ┤##
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

///
/// Load a custom gaiji (special character image 外字) for use in dialogue or UI rendering.
/// Also see wiki: https://zh.wikipedia.org/zh-hk/%E5%A4%96%E5%AD%97
///
/// Arg1: path to the gaiji image folder (string)
///       - Example: "graph/gaiji_ikari"
/// Arg2: size slot (integer 12–64)
///       - Corresponds to the text size or rendering slot where this gaiji will be used
///       - Different slots allow the same character to appear at multiple sizes in dialogue
/// Arg3: keyword (string)
///       - The character or symbol this gaiji represents
///       - Example: "怒", "汗", "汁", "ハ"
///
/// Notes:
/// - The gaiji image is mapped to the keyword in the engine’s gaiji table for the given size.
///
/// Example usage (script):
/// ```text
/// GaijiLoad("graph/gaiji_ikari", 28, "怒")   // Slot 28, kanji for "anger"
/// GaijiLoad("graph/gaiji_ase", 28, "汗")    // Slot 28, kanji for "sweat"
/// GaijiLoad("graph/gaiji_ikari156", 39, "怒") // Slot 39, larger size
/// GaijiLoad("graph/gaiji_heart68", 17, "ハ") // Slot 17, small heart symbol
/// ```
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

