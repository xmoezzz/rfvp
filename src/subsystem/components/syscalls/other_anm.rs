use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

pub enum DissolveType {
    // no animation
    None = 0,
    Static = 1,
    ColoredFadeIn = 2,
    ColoredFadeOut = 3,
    MaskFadeIn = 4,
    MaskFadeInOut = 5,
    MaskFadeOut = 6,
}

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

    match name_or_color {
        Variant::ConstString(s, _) | Variant::String(s) => {
            
        },
        Variant::Int(color_id) => {

        },
        _ => bail!("dissolve: invalid name_or_color type"),
    }
    Ok(Variant::Nil)
}
