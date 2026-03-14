use anyhow::Result;

use crate::subsystem::world::GameData;
use crate::script::Variant;
use crate::script::global::GLOBAL;
use crate::subsystem::resources::input_manager::KeyCode;

use super::{get_var, Syscaller};

pub fn text_buff(
    game_data: &mut GameData,
    id: &Variant,
    w: &Variant,
    h: &Variant,
) -> Result<Variant> {
    // IDA (TextBuff):
    // - Requires args[0] Int in [0, 31]
    // - Defaults: w=8, h=8
    // - If args[1]/args[2] are Int and non-negative, they override w/h
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_buff: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_buff: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let mut ww: i32 = 8;
    let mut hh: i32 = 8;

    if let Variant::Int(v) = w {
        if *v >= 0 {
            ww = *v;
        }
    }
    if let Variant::Int(v) = h {
        if *v >= 0 {
            hh = *v;
        }
    }

    game_data.motion_manager.text_manager.set_text_buff(id, ww, hh);

    // Upload the cleared buffer to Graph(4064 + slot) immediately.
    let _ = game_data
        .motion_manager
        .text_upload_slot(id, &game_data.fontface_manager, false);

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
    let _ = game_data
        .motion_manager
        .text_upload_slot(id, &game_data.fontface_manager, false);
    Ok(Variant::Nil)
}

pub fn text_color(
    game_data: &mut GameData,
    id: &Variant,
    color1_id: &Variant,
    color2_id: &Variant,
    color3_id: &Variant,
) -> Result<Variant> {
    // IDA (TextColor): each color argument is optional; only applies when (Type==Int && value < 256).
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_color: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_color: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    if let Variant::Int(cid) = color1_id {
        if (0..256).contains(cid) {
            let color = game_data.motion_manager.color_manager.get_entry(*cid as u8);
            game_data.motion_manager.text_manager.set_text_color1(id, color);
        }
    }

    if let Variant::Int(cid) = color2_id {
        if (0..256).contains(cid) {
            let color = game_data.motion_manager.color_manager.get_entry(*cid as u8);
            game_data.motion_manager.text_manager.set_text_color2(id, color);
        }
    }

    if let Variant::Int(cid) = color3_id {
        if (0..256).contains(cid) {
            let color = game_data.motion_manager.color_manager.get_entry(*cid as u8);
            game_data.motion_manager.text_manager.set_text_color3(id, color);
        }
    }

    Ok(Variant::Nil)
}


// -1 current font
// -2..-5 built-in Japanese fonts
pub fn text_font(
    game_data: &mut GameData,
    id: &Variant,
    font_id: &Variant,
    font_id2: &Variant,
) -> Result<Variant> {
    // IDA (TextFont):
    // - args[1] and args[2] are optional ints:
    //     if (Type==2 && -5 <= v <= max_font_idx) set_font_idx1/2(...)
    // - If an argument is Nil/other, it is ignored (keep previous value).
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_font: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_font: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    // In our FontfaceManager, user-loaded fonts are 1..=count; built-ins are negative ids.
    // IDA computes a max index for the current font enumeration; we approximate it as the
    // maximum positive id we can address (== count).
    let max_font_idx = game_data.fontface_manager.get_font_count();

    if let Variant::Int(fid) = font_id {
        if *fid >= -5 && *fid <= max_font_idx {
            game_data
                .motion_manager
                .text_manager
                .set_text_font_idx1(id, *fid);
        }
    }

    if let Variant::Int(fid2) = font_id2 {
        if *fid2 >= -5 && *fid2 <= max_font_idx {
            game_data
                .motion_manager
                .text_manager
                .set_text_font_idx2(id, *fid2);
        }
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

    if id >= -5 && id <= game_data.fontface_manager.get_font_count() {
        if let Some(font_name) = game_data.fontface_manager.get_font_name(id) {
            game_data.fontface_manager.set_system_fontface_id(id);
            game_data.fontface_manager.set_current_font_name(&font_name);
        }
    }
    Ok(Variant::Nil)
}

pub fn text_format(
    game_data: &mut GameData,
    id: &Variant,
    space_vertical: &Variant,
    space_horizon: &Variant,
    text_start_vertical: &Variant,
    text_start_horizon: &Variant,
    ruby_horizon: &Variant,
    ruby_vertical: &Variant,
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

    let space_vertical = match space_vertical {
        Variant::Int(v) if (-32..=32).contains(v) => Some(*v as i16),
        _ => None,
    };
    let space_horizon = match space_horizon {
        Variant::Int(v) if (-32..=32).contains(v) => Some(*v as i16),
        _ => None,
    };
    let text_start_vertical = match text_start_vertical {
        Variant::Int(v) if (0..=64).contains(v) => Some(*v as u16),
        _ => None,
    };
    let text_start_horizon = match text_start_horizon {
        Variant::Int(v) if (0..=64).contains(v) => Some(*v as u16),
        _ => None,
    };
    let ruby_horizon = match ruby_horizon {
        Variant::Int(v) if (-16..=16).contains(v) => Some(*v as i16),
        _ => None,
    };
    let ruby_vertical = match ruby_vertical {
        Variant::Int(v) if (-16..=16).contains(v) => Some(*v as i16),
        _ => None,
    };

    game_data.motion_manager.text_manager.apply_text_format(
        id,
        space_vertical,
        space_horizon,
        text_start_vertical,
        text_start_horizon,
        ruby_horizon,
        ruby_vertical,
    );

    Ok(Variant::Nil)
}


pub fn text_function(
    game_data: &mut GameData,
    id: &Variant,
    special_unit_mode: &Variant,
    ruby_mode: &Variant,
    wait_mode: &Variant,
) -> Result<Variant> {
    // Reverse-engineered TextFunction mapping:
    //   arg1 -> <...> special-unit mode    (0..1)
    //   arg2 -> [...] ruby parser mode     (0..2)
    //   arg3 -> {n} wait-control mode      (0..2)
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_function: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_function: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    if let Variant::Int(v) = special_unit_mode {
        if (0..=1).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_special_unit_mode(id, *v as u8);
        }
    }

    if let Variant::Int(v) = ruby_mode {
        if (0..=2).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_ruby_mode(id, *v as u8);
        }
    }

    if let Variant::Int(v) = wait_mode {
        if (0..=2).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_wait_mode(id, *v as u8);
        }
    }

    Ok(Variant::Nil)
}


pub fn text_out_size(
    game_data: &mut GameData,
    id: &Variant,
    outline: &Variant,
    ruby_outline: &Variant,
) -> Result<Variant> {
    // IDA (TextOutSize):
    // - outline is optional int, applied when (Type==2 && value <= 12)
    // - ruby_outline is optional int, applied when (Type==2 && value <= 8)
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_out_size: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_out_size: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    if let Variant::Int(v) = outline {
        if (0..=12).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_outline1(id, *v as u8);
        }
    }

    if let Variant::Int(v) = ruby_outline {
        if (0..=8).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_outline2(id, *v as u8);
        }
    }

    Ok(Variant::Nil)
}


pub fn text_pause(game_data: &mut GameData, id: &Variant, pause: &Variant) -> Result<Variant> {
    // IDA (TextPause):
    // - If pause is Int and <= 1: set is_suspended
    // - Else if pause is Nil: return current is_suspended as Int (0/1)
    // - Other types/values: no side effects
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_pause: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_pause: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    match pause {
        Variant::Int(v) if *v == 0 || *v == 1 => {
            game_data
                .motion_manager
                .text_manager
                .set_text_suspend(id, *v != 0);
            Ok(Variant::Nil)
        }
        Variant::Nil => {
            let paused = game_data.motion_manager.text_manager.get_text_suspend(id);
            Ok(Variant::Int(if paused { 1 } else { 0 }))
        }
        _ => Ok(Variant::Nil),
    }
}


pub fn text_pos(
    game_data: &mut GameData,
    id: &Variant,
    x: &Variant,
    y: &Variant,
) -> Result<Variant> {
    // IDA (TextPos): x/y are optional ints; non-int (including Nil) means "keep previous".
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_pos: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_pos: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    if let Variant::Int(vx) = x {
        game_data
            .motion_manager
            .text_manager
            .set_text_pos_x(id, *vx as u16);
    }

    if let Variant::Int(vy) = y {
        game_data
            .motion_manager
            .text_manager
            .set_text_pos_y(id, *vy as u16);
    }

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
            if s.len() >= 512 {
                log::error!("text_print: content length >= 512 is not supported");
                return Ok(Variant::Nil);
            }
            game_data.motion_manager.text_manager.set_text_content(id, s);
            let _ = game_data
                .motion_manager
                .text_upload_slot(id, &game_data.fontface_manager, true);
            let ctrl_down = (game_data.inputs_manager.get_input_state() & (1u32 << (KeyCode::Ctrl as u32))) != 0;
            let pulse = game_data.inputs_manager.peek_control_pulse();
            let global0 = GLOBAL.lock().unwrap().get_int_var(0);
            if game_data
                .motion_manager
                .text_manager
                .should_block_on_print(id, global0, ctrl_down, pulse)
            {
                let tid = game_data.get_current_thread();
                game_data
                    .motion_manager
                    .text_manager
                    .arm_sync_print_wait(id, tid);
                game_data.thread_wrapper.thread_text_wait(tid);
                game_data.thread_wrapper.should_break();
            }
            Ok(Variant::Nil)
        }
        Variant::ConstString(s, addr) => {
            if s.len() >= 512 {
                log::error!("text_print: content length >= 512 is not supported");
                return Ok(Variant::Nil);
            }
            // Const-string prints like a normal string AND marks a bitmap by its offset.
            // IMPORTANT: without uploading the updated slot buffer, the visible text stays stale
            // (typically still the cleared TextBuff), which makes the message window appear empty.
            game_data.motion_manager.text_manager.set_text_content(id, s);
            let _ = game_data
                .motion_manager
                .text_upload_slot(id, &game_data.fontface_manager, true);
            let ctrl_down = (game_data.inputs_manager.get_input_state() & (1u32 << (KeyCode::Ctrl as u32))) != 0;
            let pulse = game_data.inputs_manager.peek_control_pulse();
            let global0 = GLOBAL.lock().unwrap().get_int_var(0);
            if game_data
                .motion_manager
                .text_manager
                .should_block_on_print(id, global0, ctrl_down, pulse)
            {
                let tid = game_data.get_current_thread();
                game_data
                    .motion_manager
                    .text_manager
                    .arm_sync_print_wait(id, tid);
                game_data.thread_wrapper.thread_text_wait(tid);
                game_data.thread_wrapper.should_break();
            }

            let first = game_data.motion_manager.text_manager.mark_readed_text_first(*addr);
            if first {
                Ok(Variant::True)
            } else {
                Ok(Variant::Nil)
            }
        }
        _ => {
            log::error!("text_print: invalid content type");
            Ok(Variant::Nil)
        }
    }
}

pub fn text_reprint(game_data: &mut GameData) -> Result<Variant> {
    game_data.motion_manager.text_reprint(&game_data.fontface_manager);
    Ok(Variant::Nil)
}

pub fn text_shadow_dist(game_data: &mut GameData, id: &Variant, dist: &Variant) -> Result<Variant> {
    // IDA (TextShadowDist):
    // - Default distance is 0
    // - If arg is Int: clamp to [0, 12] (negative -> 0, >12 -> 12)
    // - Non-int (including Nil) keeps default 0 and still applies it.
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_shadow_dist: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_shadow_dist: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    let mut d: i32 = 0;
    if let Variant::Int(v) = dist {
        d = *v;
    }

    if d < 0 {
        d = 0;
    } else if d > 12 {
        d = 12;
    }

    game_data
        .motion_manager
        .text_manager
        .set_text_shadow_dist(id, d as u8);

    Ok(Variant::Nil)
}


pub fn text_size(
    game_data: &mut GameData,
    id: &Variant,
    size: &Variant,
    ruby_size: &Variant,
) -> Result<Variant> {
    // IDA (TextSize):
    // - size is optional int in [12, 64]
    // - ruby_size is optional int in [8, 32]
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_size: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_size: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    if let Variant::Int(v) = size {
        if (12..=64).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_size1(id, *v as u8);
        }
    }

    if let Variant::Int(v) = ruby_size {
        if (8..=32).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_size2(id, *v as u8);
        }
    }

    Ok(Variant::Nil)
}


pub fn text_skip(game_data: &mut GameData, id: &Variant, skip: &Variant) -> Result<Variant> {
    // IDA (TextSkip): skip is optional int; only applies when skip < 4.
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_skip: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_skip: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    if let Variant::Int(v) = skip {
        if (0..=3).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_skip(id, *v as u8);
        }
    }

    Ok(Variant::Nil)
}


pub fn text_space(
    game_data: &mut GameData,
    id: &Variant,
    space_vertical: &Variant,
    space_horizon: &Variant,
) -> Result<Variant> {
    // IDA (TextSpace):
    // - space_vertical is optional int in [-32, 32]
    // - space_horizon is optional int in [-32, 32]
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_space: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_space: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    if let Variant::Int(v) = space_vertical {
        if (-32..=32).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_vertical_space(id, *v as i16);
        }
    }

    if let Variant::Int(v) = space_horizon {
        if (-32..=32).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_horizon_space(id, *v as i16);
        }
    }

    Ok(Variant::Nil)
}


/// set the text speed for the corresponding text id
/// 0 is for immediate display
pub fn text_speed(game_data: &mut GameData, id: &Variant, speed: &Variant) -> Result<Variant> {
    // IDA (TextSpeed): speed is optional int in [-1, 300000].
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("text_speed: invalid id type");
            return Ok(Variant::Nil);
        }
    };

    if !(0..32).contains(&id) {
        log::error!("text_speed: id should be in range 0..32");
        return Ok(Variant::Nil);
    }

    if let Variant::Int(v) = speed {
        if (-1..=300000).contains(v) {
            game_data
                .motion_manager
                .text_manager
                .set_text_speed(id, *v);
        }
    }

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

    // IDA behavior: return True only when the bit transitions 0 -> 1.
    if game_data.motion_manager.text_manager.mark_readed_text_first(addr) {
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



/// TextRepaint()
/// IDA SYSCALL_SPECS: argc=0
pub struct TextRepaint;
impl super::Syscaller for TextRepaint {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        // Re-upload all text slots (GPU textures) without changing content.
        game_data.motion_manager.text_reprint(&game_data.fontface_manager);
        Ok(Variant::Nil)
    }
}
