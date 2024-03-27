use anyhow::Result;

use crate::subsystem::world::GameData;
use crate::{script::Variant, subsystem::resources::text_manager::FONTFACE_MS_GOTHIC};

use super::{get_var, Syscaller};

pub fn text_buff(
    game_data: &mut GameData,
    id: &Variant,
    w: &Variant,
    h: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_buff: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_buff: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let w = if let Variant::Int(w) = w {
        if *w < 0 {
            8
        } else {
            *w
        }
    } else {
        log::error!("text_buff: invalid w type");
        return Ok(Variant::Nil);
    };

    let h = if let Variant::Int(h) = h {
        if *h < 0 {
            8
        } else {
            *h
        }
    } else {
        log::error!("text_buff: invalid h type");
        return Ok(Variant::Nil);
    };

    game_data
        .motion_manager
        .text_manager
        .set_text_buff(id, w, h);
    Ok(Variant::Nil)
}

pub fn text_clear(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_clear: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_clear: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.text_manager.set_text_clear(id);
    Ok(Variant::Nil)
}

pub fn text_color(
    game_data: &mut GameData,
    id: &Variant,
    color1_id: &Variant,
    color2_id: &Variant,
    color3_id: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_color: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_color: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let color1_id = match color1_id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_color: invalid color1_id type");
            return Ok(Variant::Nil);
        },
    };

    if (0..256).contains(&color1_id) {
        let color = game_data.color_manager.get_entry(color1_id as u8);
        game_data
            .motion_manager
            .text_manager
            .set_text_color1(id, color);
    }

    let color2_id = match color2_id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_color: invalid color2_id type");
            return Ok(Variant::Nil);
        },
    };

    if (0..256).contains(&color2_id) {
        let color = game_data.color_manager.get_entry(color2_id as u8);
        game_data
            .motion_manager
            .text_manager
            .set_text_color2(id, color);
    }

    let color3_id = match color3_id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_color: invalid color3_id type");
            return Ok(Variant::Nil);
        },
    };

    if (0..256).contains(&color3_id) {
        let color = game_data.color_manager.get_entry(color3_id as u8);
        game_data
            .motion_manager
            .text_manager
            .set_text_color3(id, color);
    }

    Ok(Variant::Nil)
}

// ＭＳ ゴシック
// ＭＳ 明朝
// ＭＳ Ｐゴシック
// ＭＳ Ｐ明朝
pub fn text_font(
    game_data: &mut GameData,
    id: &Variant,
    font_id: &Variant,
    font_id2: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_font: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_font: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let font_id = match font_id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_font: invalid font_id type");
            return Ok(Variant::Nil);
        },
    };

    let max_count = game_data.fontface_manager.get_font_count();

    if font_id >= -5 && font_id < max_count && max_count != 0 {
        game_data
            .motion_manager
            .text_manager
            .set_font_name(id, font_id);
    } else {
        game_data
            .motion_manager
            .text_manager
            .set_font_name(id, FONTFACE_MS_GOTHIC);
    }

    let font_id2 = match font_id2 {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_font: invalid font_id2 type");
            return Ok(Variant::Nil);
        },
    };

    if font_id2 >= -5 && font_id2 < max_count && max_count != 0 {
        game_data
            .motion_manager
            .text_manager
            .set_font_text(id, font_id2);
    } else {
        game_data
            .motion_manager
            .text_manager
            .set_font_text(id, FONTFACE_MS_GOTHIC);
    }

    Ok(Variant::Nil)
}

pub fn text_font_count(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.fontface_manager.get_font_count()))
}

pub fn text_font_get(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(
        game_data.fontface_manager.get_system_fontface_id(),
    ))
}

pub fn text_font_name(game_data: &GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_font_name: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    match game_data.fontface_manager.get_font_name(id) {
        Some(name) => Ok(Variant::String(name)),
        None => Ok(Variant::Nil),
    }
}

pub fn text_font_set(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_set_font: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if id >= -5 && id < game_data.fontface_manager.get_font_count() {
        if let Some(font_name) = game_data.fontface_manager.get_font_name(id) {
            game_data.fontface_manager.set_system_fontface_id(id);
            game_data.fontface_manager.set_current_font_name(&font_name)
        }
    } else {
        game_data
            .fontface_manager
            .set_system_fontface_id(FONTFACE_MS_GOTHIC);
        game_data
            .fontface_manager
            .set_current_font_name("ＭＳ ゴシック");
    }
    game_data.fontface_manager.set_system_fontface_id(id);
    Ok(Variant::Nil)
}

// pub fn text_format(
//     game_data: &mut GameData,
//     id: &Variant,

//     )

pub fn text_format(
    game_data: &mut GameData,
    id: &Variant,
    space_vertical: &Variant,
    space_horizon: &Variant,
    text_start_vertical: &Variant,
    text_start_horizon: &Variant,
    ruby_vertical: &Variant,
    ruby_horizon: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_format: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_format: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    log::warn!("text_format: not implemented");

    Ok(Variant::Nil)
}

pub fn text_function(
    game_data: &mut GameData,
    id: &Variant,
    func1: &Variant,
    func2: &Variant,
    func3: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_function: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_function: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let func1 = match func1 {
        Variant::Int(func1) => *func1,
        _ => {
            log::error!("text_function: invalid func type");
            return Ok(Variant::Nil);
        },
    };

    if (0..=1).contains(&func1) {
        game_data
            .motion_manager
            .text_manager
            .set_text_function1(id, func1);
    }

    let func2 = match func2 {
        Variant::Int(func2) => *func2,
        _ => {
            log::error!("text_function: invalid func type");
            return Ok(Variant::Nil);
        },
    };

    if (0..=2).contains(&func2) {
        game_data
            .motion_manager
            .text_manager
            .set_text_function2(id, func2);
    }

    let func3 = match func3 {
        Variant::Int(func3) => *func3,
        _ => {
            log::error!("text_function: invalid func type");
            return Ok(Variant::Nil);
        },
    };

    if (0..=2).contains(&func3) {
        game_data
            .motion_manager
            .text_manager
            .set_text_function3(id, func3);
    }

    Ok(Variant::Nil)
}

pub fn text_out_size(
    game_data: &mut GameData,
    id: &Variant,
    outline: &Variant,
    ruby_outline: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_out_size: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_out_size: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let outline = match outline {
        Variant::Int(outline) => *outline,
        _ => {
            log::error!("text_out_size: invalid outline type");
            return Ok(Variant::Nil);
        },
    };

    if (0..=12).contains(&outline) {
        game_data
            .motion_manager
            .text_manager
            .set_text_outline(id, outline);
    }

    let ruby_outline = match ruby_outline {
        Variant::Int(outline) => *outline,
        _ => {
            log::error!("text_out_size: invalid ruby_outline type");
            return Ok(Variant::Nil);
        },
    };

    if (0..=8).contains(&ruby_outline) {
        game_data
            .motion_manager
            .text_manager
            .set_text_ruby_outline(id, ruby_outline);
    }

    Ok(Variant::Nil)
}

pub fn text_pause(game_data: &mut GameData, id: &Variant, pause: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_pause: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_pause: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    match pause {
        Variant::Int(pause) => {
            let pause = *pause != 0;
            game_data
                .motion_manager
                .text_manager
                .set_text_suspend(id, pause);
        }
        Variant::Nil => {
            let paused = game_data.motion_manager.text_manager.get_text_suspend(id);

            // convert bool to int
            if paused {
                return Ok(Variant::Int(1));
            } else {
                return Ok(Variant::Int(0));
            }
        }
        _ => {
            log::error!("text_pause: invalid pause type");
            return Ok(Variant::Nil);
        },
    };

    Ok(Variant::Nil)
}

pub fn text_pos(
    game_data: &mut GameData,
    id: &Variant,
    x: &Variant,
    y: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_pos: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_pos: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let x = match x {
        Variant::Int(x) => *x,
        _ => {
            log::error!("text_pos: invalid x type");
            return Ok(Variant::Nil);
        },
    };

    game_data.motion_manager.text_manager.set_text_pos_x(id, x);

    let y = match y {
        Variant::Int(y) => *y,
        _ => {
            log::error!("text_pos: invalid y type");
            return Ok(Variant::Nil);
        },
    };

    game_data.motion_manager.text_manager.set_text_pos_y(id, y);

    Ok(Variant::Nil)
}

pub fn text_print(game_data: &mut GameData, id: &Variant, content: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_print: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_print: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    match content {
        Variant::String(s) => {
            game_data
                .motion_manager
                .text_manager
                .set_text_content(id, s);
        }
        Variant::ConstString(_, addr) => {
            let addr = *addr;
            game_data.motion_manager.text_manager.set_readed_text(addr);
        }
        _ => {
            log::error!("text_print: invalid content type");
            return Ok(Variant::Nil);
        },
    }

    Ok(Variant::Nil)
}

pub fn text_reprint(game_data: &mut GameData) -> Result<Variant> {
    game_data.motion_manager.text_reprint();
    Ok(Variant::Nil)
}

pub fn text_shadow_dist(game_data: &mut GameData, id: &Variant, dist: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_shadow_dist: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_shadow_dist: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let dist = match dist {
        Variant::Int(dist) => {
            if !(0..=12).contains(dist) {
                if *dist < 0 {
                    0
                } else {
                    12
                }
            } else {
                *dist
            }
        }
        _ => {
            log::error!("text_shadow_dist: invalid dist type");
            return Ok(Variant::Nil);
        },
    };

    if (0..=12).contains(&dist) {
        game_data
            .motion_manager
            .text_manager
            .set_text_shadow_distance(id, dist);
    }

    Ok(Variant::Nil)
}

pub fn text_size(
    game_data: &mut GameData,
    id: &Variant,
    size: &Variant,
    ruby_size: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_size: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_size: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let size = match size {
        Variant::Int(size) => *size,
        _ => {
            log::error!("text_size: invalid size type");
            return Ok(Variant::Nil);
        },
    };

    if (12..=64).contains(&size) {
        game_data
            .motion_manager
            .text_manager
            .set_text_main_text_size(id, size);
    }

    let ruby_size = match ruby_size {
        Variant::Int(size) => *size,
        _ => {
            log::error!("text_size: invalid ruby_size type");
            return Ok(Variant::Nil);
        },
    };

    if (8..=32).contains(&ruby_size) {
        game_data
            .motion_manager
            .text_manager
            .set_text_ruby_text_size(id, ruby_size);
    }

    Ok(Variant::Nil)
}

pub fn text_skip(game_data: &mut GameData, id: &Variant, skip: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_skip: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_skip: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let skip = match skip {
        Variant::Int(skip) => *skip,
        _ => {
            log::error!("text_skip: invalid skip type");
            return Ok(Variant::Nil);
        },
    };

    if (0..=3).contains(&skip) {
        game_data
            .motion_manager
            .text_manager
            .set_text_skip(id, skip);
    }

    Ok(Variant::Nil)
}

pub fn text_space(
    game_data: &mut GameData,
    id: &Variant,
    space_vertical: &Variant,
    space_horizon: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_space: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_space: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let space_vertical = match space_vertical {
        Variant::Int(space) => *space,
        _ => {
            log::error!("text_space: invalid space_vertical type");
            return Ok(Variant::Nil);
        },
    };

    let space_horizon = match space_horizon {
        Variant::Int(space) => *space,
        _ => {
            log::error!("text_space: invalid space_horizon type");
            return Ok(Variant::Nil);
        },
    };

    if (-32..=32).contains(&space_vertical) {
        game_data
            .motion_manager
            .text_manager
            .set_text_space_vertical(id, space_vertical);
    }

    if (-32..=32).contains(&space_horizon) {
        game_data
            .motion_manager
            .text_manager
            .set_text_space_horizon(id, space_horizon);
    }

    Ok(Variant::Nil)
}

/// set the text speed for the corresponding text id
/// 0 is for immediate display
pub fn text_speed(game_data: &mut GameData, id: &Variant, speed: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_speed: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_speed: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let speed = match speed {
        Variant::Int(speed) => *speed,
        _ => {
            log::error!("text_speed: invalid speed type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..=300000).contains(&speed) {
        log::error!("text_speed: speed should be in range 0..300000");
        return Ok(Variant::Nil);
    }

    game_data
        .motion_manager
        .text_manager
        .set_text_speed(id, speed);
    Ok(Variant::Nil)
}

/// set the kinsoku chars (禁则) for the corresponding text id
pub fn text_suspend_chr(game_data: &mut GameData, id: &Variant, chrs: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_suspend_chr: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..32).contains(&id) {
        log::error!("text_suspend_chr: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let chrs = match chrs {
        Variant::String(chrs) | Variant::ConstString(chrs, _) => chrs.clone(),
        _ => {
            log::error!("text_suspend_chr: invalid chrs type");
            return Ok(Variant::Nil);
        },
    };

    game_data
        .motion_manager
        .text_manager
        .set_text_suspend_chr(id, &chrs);
    Ok(Variant::Nil)
}

/// test the const string was readed
/// the original implementation use a bitmap to record the text address
pub fn text_test(game_data: &mut GameData, const_string: &Variant) -> Result<Variant> {
    let addr = match const_string {
        Variant::ConstString(_, addr) => *addr,
        _ => return Ok(Variant::Nil),
    };

    if game_data.motion_manager.text_manager.test_readed_text(addr) {
        Ok(Variant::True)
    } else {
        Ok(Variant::Nil)
    }
}

pub struct TextBuff;
impl Syscaller for TextBuff {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_buff(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
        )
    }
}

unsafe impl Send for TextBuff {}
unsafe impl Sync for TextBuff {}

pub struct TextClear;
impl Syscaller for TextClear {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_clear(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for TextClear {}
unsafe impl Sync for TextClear {}

pub struct TextColor;
impl Syscaller for TextColor {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_color(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
            get_var!(args, 3),
        )
    }
}

unsafe impl Send for TextColor {}
unsafe impl Sync for TextColor {}


pub struct TextFont;
impl Syscaller for TextFont {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_font(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
        )
    }
}

unsafe impl Send for TextFont {}
unsafe impl Sync for TextFont {}

pub struct TextFontCount;
impl Syscaller for TextFontCount {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        text_font_count(game_data)
    }
}

unsafe impl Send for TextFontCount {}
unsafe impl Sync for TextFontCount {}


pub struct TextFontGet;
impl Syscaller for TextFontGet {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        text_font_get(game_data)
    }
}

unsafe impl Send for TextFontGet {}
unsafe impl Sync for TextFontGet {}


pub struct TextFontName;
impl Syscaller for TextFontName {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_font_name(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for TextFontName {}
unsafe impl Sync for TextFontName {}


pub struct TextFontSet;
impl Syscaller for TextFontSet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_font_set(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for TextFontSet {}
unsafe impl Sync for TextFontSet {}


pub struct TextFormat;
impl Syscaller for TextFormat {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_format(
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


unsafe impl Send for TextFormat {}
unsafe impl Sync for TextFormat {}


pub struct TextFunction;
impl Syscaller for TextFunction {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_function(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
            get_var!(args, 3),
        )
    }
}


unsafe impl Send for TextFunction {}
unsafe impl Sync for TextFunction {}


pub struct TextOutSize;
impl Syscaller for TextOutSize {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_out_size(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
        )
    }
}


unsafe impl Send for TextOutSize {}
unsafe impl Sync for TextOutSize {}


pub struct TextPause;
impl Syscaller for TextPause {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_pause(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for TextPause {}
unsafe impl Sync for TextPause {}


pub struct TextPos;
impl Syscaller for TextPos {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_pos(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
        )
    }
}

unsafe impl Send for TextPos {}
unsafe impl Sync for TextPos {}


pub struct TextPrint;
impl Syscaller for TextPrint {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_print(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for TextPrint {}
unsafe impl Sync for TextPrint {}


pub struct TextReprint;
impl Syscaller for TextReprint {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        text_reprint(game_data)
    }
}

unsafe impl Send for TextReprint {}
unsafe impl Sync for TextReprint {}


pub struct TextShadowDist;
impl Syscaller for TextShadowDist {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_shadow_dist(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
        )
    }
}


unsafe impl Send for TextShadowDist {}
unsafe impl Sync for TextShadowDist {}


pub struct TextSize;
impl Syscaller for TextSize {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_size(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
        )
    }
}

unsafe impl Send for TextSize {}
unsafe impl Sync for TextSize {}


pub struct TextSkip;
impl Syscaller for TextSkip {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_skip(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
        )
    }
}

unsafe impl Send for TextSkip {}
unsafe impl Sync for TextSkip {}


pub struct TextSpace;
impl Syscaller for TextSpace {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_space(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
            get_var!(args, 2),
        )
    }
}

unsafe impl Send for TextSpace {}
unsafe impl Sync for TextSpace {}


pub struct TextSpeed;
impl Syscaller for TextSpeed {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_speed(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
        )
    }
}

unsafe impl Send for TextSpeed {}
unsafe impl Sync for TextSpeed {}


pub struct TextSuspendChr;
impl Syscaller for TextSuspendChr {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_suspend_chr(
            game_data,
            get_var!(args, 0),
            get_var!(args, 1),
        )
    }
}

unsafe impl Send for TextSuspendChr {}
unsafe impl Sync for TextSuspendChr {}


pub struct TextTest;
impl Syscaller for TextTest {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        text_test(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for TextTest {}
unsafe impl Sync for TextTest {}

