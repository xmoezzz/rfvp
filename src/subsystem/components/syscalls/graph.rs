use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::resources::prim::PrimType;
use crate::subsystem::world::GameData;


pub fn prim_exit_group(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_exit_group: invalid id : {:?}", id),
    };

    if !(0..=4095).contains(&id) {
        bail!("prim_exit_group: invalid id : {}", id);
    }

    Ok(Variant::Nil)
}

pub fn prim_group_in(game_data: &mut GameData, id: &Variant, id2: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_group_in: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_group_in: invalid id : {}", id);
    }

    let id2 = match id2.as_int() {
        Some(id2) => id2,
        None => bail!("prim_group_in: invalid id2 : {:?}", id2),
    };

    if !(0..=4095).contains(&id2) {
        bail!("prim_group_in: invalid id2 : {}", id2);
    }

    game_data.prim_manager.set_prim_group_in(id2, id);

    Ok(Variant::Nil)
}

pub fn prim_group_move(game_data: &mut GameData, id: &Variant, id2: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_group_move: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_group_move: invalid id : {}", id);
    }

    let id2 = match id2.as_int() {
        Some(id2) => id2,
        None => bail!("prim_group_move: invalid id2 : {:?}", id2),
    };

    if !(1..=4095).contains(&id2) {
        bail!("prim_group_move: invalid id2 : {}", id2);
    }

    game_data.prim_manager.prim_move(id2, id);

    Ok(Variant::Nil)
}

pub fn prim_group_out(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_group_out: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_group_out: invalid id : {}", id);
    }

    game_data.prim_manager.unlink_prim(id as i16);

    Ok(Variant::Nil)
}

/// reset the primitive
pub fn prim_set_null_parent(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_null_parent: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_null_parent: invalid id : {}", id);
    }

    game_data.prim_manager.prim_init_with_type(id as i16, PrimType::PrimTypeNone);

    Ok(Variant::Nil)
}

/// set the primitive's alpha value
pub fn prim_set_alpha(game_data: &mut GameData, id: &Variant, alpha: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_alpha: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_alpha: invalid id : {}", id);
    }

    let alpha = match alpha.as_int() {
        Some(alpha) => alpha,
        None => bail!("prim_set_alpha: invalid alpha : {:?}", alpha),
    };

    if !(0..=255).contains(&alpha) {
        bail!("prim_set_alpha: invalid alpha : {}", alpha);
    }

    game_data.prim_manager.prim_set_alpha(id, alpha);

    Ok(Variant::Nil)
}

pub fn prim_set_blend(game_data: &mut GameData, id: &Variant, blend: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_blend: invalid id : {:?}", id),
    };

    if !(0..=4095).contains(&id) {
        bail!("prim_set_blend: invalid id : {}", id);
    }

    let blend = match blend.as_int() {
        Some(blend) => blend,
        None => bail!("prim_set_blend: invalid blend : {:?}", blend),
    };

    if !(0..=1).contains(&blend) {
        bail!("prim_set_blend: invalid blend : {}", blend);
    }

    game_data.prim_manager.prim_set_blend(id, blend);

    Ok(Variant::Nil)
}

pub fn prim_set_draw(game_data: &mut GameData, id: &Variant, draw: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_draw: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_draw: invalid id : {}", id);
    }

    let draw = match draw.as_int() {
        Some(draw) => draw,
        None => bail!("prim_set_draw: invalid draw : {:?}", draw),
    };

    if !(0..=1).contains(&draw) {
        bail!("prim_set_draw: invalid draw : {}", draw);
    }

    game_data.prim_manager.prim_set_draw(id, draw);

    Ok(Variant::Nil)
}

// set the primitive's archor point
pub fn prim_set_op(game_data: &mut GameData, id: &Variant, opx: &Variant, opy: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_op: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_op: invalid id : {}", id);
    }

    let opx = match opx.as_int() {
        Some(op) => op,
        None => bail!("prim_set_op: invalid op : {:?}", opx),
    };

    let opy = match opy.as_int() {
        Some(op) => op,
        None => bail!("prim_set_op: invalid op : {:?}", opy),
    };

    game_data.prim_manager.prim_set_op(id, opx, opy);

    Ok(Variant::Nil)
}

/// set the primitive's rotation and scale, and the scale value is the same in x and y
pub fn prim_set_rs(game_data: &mut GameData, id: &Variant, rotation: &Variant, scale: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_rs: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_rs: invalid id : {}", id);
    }

    let rotation = match rotation.as_int() {
        Some(r) => {
            let r2 = r % 3600;
            if r2 < 0 {
                r2 + 3600
            } else {
                r2
            }
        },
        None => bail!("prim_set_rs: invalid rs : {:?}", rotation),
    };

    let scale = match scale.as_int() {
        Some(s) => s,
        None => bail!("prim_set_rs: invalid rs : {:?}", scale),
    };

    let scale = if !(0..=10000).contains(&scale) {
        100 // default value
    }
    else {
        scale
    };

    game_data.prim_manager.prim_set_rotation(id, rotation);
    game_data.prim_manager.prim_set_scale(id, scale, scale);
    game_data.prim_manager.prim_add_attr(id, 0x40);

    Ok(Variant::Nil)
}


/// set the primitive's rotation and scale, and the scale value is different in x and y
pub fn prim_set_rs2(game_data: &mut GameData, id: &Variant, rotation: &Variant, scale_x: &Variant, scale_y: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_rs2: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_rs2: invalid id : {}", id);
    }

    let rotation = match rotation.as_int() {
        Some(r) => {
            let r2 = r % 3600;
            if r2 < 0 {
                r2 + 3600
            } else {
                r2
            }
        },
        None => bail!("prim_set_rs2: invalid rs : {:?}", rotation),
    };

    let scale_x = match scale_x.as_int() {
        Some(s) => s,
        None => bail!("prim_set_rs2: invalid rs : {:?}", scale_x),
    };

    let scale_y = match scale_y.as_int() {
        Some(s) => s,
        None => bail!("prim_set_rs2: invalid rs : {:?}", scale_y),
    };

    let scale_x = if !(0..=10000).contains(&scale_x) {
        100 // default value
    }
    else {
        scale_x
    };

    let scale_y = if !(0..=10000).contains(&scale_y) {
        100 // default value
    }
    else {
        scale_y
    };

    game_data.prim_manager.prim_set_rotation(id, rotation);
    game_data.prim_manager.prim_set_scale(id, scale_x, scale_y);
    game_data.prim_manager.prim_add_attr(id, 0x40);

    Ok(Variant::Nil)
}


pub fn prim_set_snow(game_data: &mut GameData, id: &Variant, mode: &Variant, x: &Variant, y: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_snow: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_snow: invalid id : {}", id);
    }

    let mode = match mode.as_int() {
        Some(mode) => mode,
        None => bail!("prim_set_snow: invalid mode : {:?}", mode),
    };

    if !(0..=1).contains(&mode) {
        bail!("prim_set_snow: invalid mode : {}", mode);
    }

    let x = x.as_int().unwrap_or(0);
    let y = y.as_int().unwrap_or(0);

    game_data.prim_manager.prim_init_with_type(id as i16, PrimType::PrimTypeSnow);
    game_data.prim_manager.prim_set_op(id, 0, 0);
    game_data.prim_manager.prim_set_alpha(id, 255i32);
    game_data.prim_manager.prim_set_blend(id, 0);
    game_data.prim_manager.prim_set_rotation(id, 0);
    game_data.prim_manager.prim_set_scale(id, 1000, 1000);
    game_data.prim_manager.prim_set_uv(id, 0, 0);
    game_data.prim_manager.prim_set_size(id, 0, 0);
    game_data.prim_manager.prim_set_pos(id, x, y);
    game_data.prim_manager.prim_set_z(id, 1000);
    game_data.prim_manager.prim_set_attr(id, 0);

    Ok(Variant::Nil)
}

pub fn prim_set_sprt(game_data: &mut GameData, id: &Variant, src_id: &Variant, x: &Variant, y: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_snow: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_snow: invalid id : {}", id);
    }

    let src_id = match src_id.as_int() {
        Some(src_id) => src_id,
        None => bail!("prim_set_snow: invalid src_id : {:?}", src_id),
    };

    if !(-2..=4095).contains(&src_id) {
        bail!("prim_set_snow: invalid src_id : {}", src_id);
    }

    let x = x.as_int().unwrap_or(0);
    let y = y.as_int().unwrap_or(0);

    game_data.prim_manager.prim_init_with_type(id as i16, PrimType::PrimTypeSprt);
    game_data.prim_manager.prim_set_op(id, 0, 0);
    game_data.prim_manager.prim_set_alpha(id, 255i32);
    game_data.prim_manager.prim_set_blend(id, 0);
    game_data.prim_manager.prim_set_rotation(id, 0);
    game_data.prim_manager.prim_set_scale(id, 1000, 1000);
    game_data.prim_manager.prim_set_uv(id, 0, 0);
    game_data.prim_manager.prim_set_size(id, 0, 0);
    game_data.prim_manager.prim_set_pos(id, x, y);
    game_data.prim_manager.prim_set_z(id, 1000);
    game_data.prim_manager.prim_set_mode(id, 0);
    game_data.prim_manager.prim_set_attr(id, 0);

    Ok(Variant::Nil)
}

pub fn prim_set_text(game_data: &mut GameData, id: &Variant, text_id: &Variant, x: &Variant, y: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_text: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_text: invalid id : {}", id);
    }

    let text_id = match text_id.as_int() {
        Some(text_id) => text_id,
        None => bail!("prim_set_text: invalid text_id : {:?}", text_id),
    };

    if !(0..=31).contains(&text_id) {
        bail!("prim_set_text: invalid text_id : {}", text_id);
    }

    let x = x.as_int().unwrap_or(0);
    let y = y.as_int().unwrap_or(0);

    game_data.prim_manager.prim_init_with_type(id as i16, PrimType::PrimTypeText);
    game_data.prim_manager.prim_set_alpha(id, 255i32);
    game_data.prim_manager.prim_set_blend(id, 0);
    game_data.prim_manager.prim_set_pos(id, x, y);
    game_data.prim_manager.prim_remove_attr(id, 0xFE);

    Ok(Variant::Nil)
}

pub fn prim_set_tile(game_data: &mut GameData, id: &Variant, tile_id: &Variant, x: &Variant, y: &Variant, w: &Variant, h: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_tile: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_tile: invalid id : {}", id);
    }

    let tile_id = match tile_id.as_int() {
        Some(tile_id) => tile_id,
        None => bail!("prim_set_tile: invalid tile_id : {:?}", tile_id),
    };

    if !(0..=31).contains(&tile_id) {
        bail!("prim_set_tile: invalid tile_id : {}", tile_id);
    }

    let x = x.as_int().unwrap_or(0);
    let y = y.as_int().unwrap_or(0);
    let w = w.as_int().unwrap_or(0);
    let h = h.as_int().unwrap_or(0);

    game_data.prim_manager.prim_init_with_type(id as i16, PrimType::PrimTypeTile);
    game_data.prim_manager.prim_set_alpha(id, 255i32);
    game_data.prim_manager.prim_set_blend(id, 0);
    game_data.prim_manager.prim_set_pos(id, x, y);
    game_data.prim_manager.prim_set_size(id, w, h);
    game_data.prim_manager.prim_set_tile(id, tile_id);

    Ok(Variant::Nil)
}

pub fn prim_set_uv(game_data: &mut GameData, id: &Variant, u: &Variant, v: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_uv: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_uv: invalid id : {}", id);
    }

    let u = match u.as_int() {
        Some(u) => u,
        None => bail!("prim_set_uv: invalid u : {:?}", u),
    };

    let v = match v.as_int() {
        Some(v) => v,
        None => bail!("prim_set_uv: invalid v : {:?}", v),
    };

    game_data.prim_manager.prim_set_uv(id, u, v);
    game_data.prim_manager.prim_add_attr(id, 1);
    game_data.prim_manager.prim_add_attr(id, 0x40);

    Ok(Variant::Nil)
}

pub fn prim_set_xy(game_data: &mut GameData, id: &Variant, x: &Variant, y: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_xy: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_xy: invalid id : {}", id);
    }

    let x = match x.as_int() {
        Some(x) => x,
        None => bail!("prim_set_xy: invalid x : {:?}", x),
    };

    let y = match y.as_int() {
        Some(y) => y,
        None => bail!("prim_set_xy: invalid y : {:?}", y),
    };

    game_data.prim_manager.prim_set_pos(id, x, y);
    game_data.prim_manager.prim_add_attr(id, 0x40);

    Ok(Variant::Nil)
}

pub fn prim_set_z(game_data: &mut GameData, id: &Variant, z: &Variant) -> Result<Variant> {
    let id = match id.as_int() {
        Some(id) => id,
        None => bail!("prim_set_z: invalid id : {:?}", id),
    };

    if !(1..=4095).contains(&id) {
        bail!("prim_set_z: invalid id : {}", id);
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
        },
        None => bail!("prim_set_z: invalid z : {:?}", z),
    };

    game_data.prim_manager.prim_set_z(id, z);
    match game_data.prim_manager.prim_get_type(id) {
        PrimType::PrimTypeNone => {},
        PrimType::PrimTypeGroup | PrimType::PrimTypeTile => {
            game_data.prim_manager.prim_add_attr(id, 0x40);
            game_data.prim_manager.prim_add_attr(id, 4);
        },
        _ => {
            game_data.prim_manager.prim_add_attr(id, 0x40);
            game_data.prim_manager.prim_remove_attr(id, 0xFB);
        },
    };

    Ok(Variant::Nil)
}