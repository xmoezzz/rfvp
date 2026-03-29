use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::movie::movie_play;
use super::saveload::{load, save_write};
use super::text::text_suspend_chr;
use super::{get_var, Syscaller};

lazy_static::lazy_static! {
    static ref LEGACY_CHR_TABLE: Mutex<Vec<LegacyChrEntry>> = Mutex::new(Vec::new());
    static ref LEGACY_TEXT_STATE: Mutex<HashMap<i32, LegacyTextState>> = Mutex::new(HashMap::new());
    static ref LEGACY_CONFIG_STATE: Mutex<LegacyConfigState> = Mutex::new(LegacyConfigState::default());
    static ref LEGACY_UI_STATE: Mutex<LegacyUiState> = Mutex::new(LegacyUiState::default());
}

#[derive(Clone, Debug, Default)]
struct LegacyChrEntry {
    _name: String,
    _color_slot: u8,
    rgba: [u8; 4],
    volume: i32,
    _voice_prefix: String,
}

#[derive(Clone, Debug, Default)]
struct LegacyTextHistoryEntry {
    slot0: Variant,
    slot1: Variant,
}

#[derive(Clone, Debug, Default)]
struct LegacyTextState {
    enabled: bool,
    prim_a: Option<i32>,
    prim_b: Option<i32>,
    pending_slot0: Variant,
    pending_slot1: Variant,
    history: Vec<LegacyTextHistoryEntry>,
}

#[derive(Clone, Debug, Default)]
struct LegacyConfigState {
    /// ConfigDisplay slots 0..8.
    ///
    /// Reverse, AngelWish:
    ///   0,1: 0..10 sliders, stored directly, UI shows 10 - value
    ///   2:   bool
    ///   3:   0..3 enum
    ///   4:   bool
    ///   5..8: 0..255 color components used by the native display-config preview tree
    display: [i32; 9],
    /// ConfigEtc slots 14..16 in the original config table.
    ///
    /// Reverse, AngelWish:
    ///   14,15: bools exposed only through the native "etc" dialog in the uploaded database
    ///   16:    bool used by the native message-skip menu label toggle (`sub_40C970` -> `sub_413340`)
    etc: [i32; 3],
    /// ConfigSound slots 9..13, one 0..100 volume per sound type.
    sound: [i32; 5],
    /// Original ConfigSet (`sub_40A6C0`) is one-shot. After the first successful apply,
    /// later calls return immediately.
    configured: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacySaveLoadRequest {
    LoadFile,
    SaveFile,
    LoadTitle,
    SaveTitle,
}

#[derive(Clone, Debug, Default)]
struct LegacyUiState {
    save_name_left: Option<String>,
    save_name_right: Option<String>,
    pending_save_load_request: Option<LegacySaveLoadRequest>,
    save_load_menu_visible: bool,
}

fn normalize_prefix(prefix: &str) -> String {
    if prefix.is_empty() {
        return String::new();
    }
    let mut s = prefix.replace('\\', "/");
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

fn get_string_arg(arg: &Variant) -> Option<String> {
    match arg {
        Variant::String(s) | Variant::ConstString(s, _) => Some(s.clone()),
        _ => None,
    }
}

fn set_pending_save_load_request(req: LegacySaveLoadRequest) {
    let mut state = LEGACY_UI_STATE.lock().unwrap();
    state.pending_save_load_request = Some(req);
    state.save_load_menu_visible = true;
}

pub fn take_pending_save_load_request() -> Option<LegacySaveLoadRequest> {
    let mut state = LEGACY_UI_STATE.lock().unwrap();
    state.pending_save_load_request.take()
}

pub fn legacy_save_load_menu_visible() -> bool {
    LEGACY_UI_STATE.lock().unwrap().save_load_menu_visible
}

pub fn set_legacy_save_load_menu_visible(visible: bool) {
    let mut state = LEGACY_UI_STATE.lock().unwrap();
    state.save_load_menu_visible = visible;
}

fn variant_truthy(arg: &Variant) -> bool {
    match arg {
        Variant::Nil => false,
        Variant::Int(v) => *v != 0,
        _ => true,
    }
}

fn sanitize_config_display_value(index: usize, arg: Option<&Variant>) -> i32 {
    let Some(v) = arg.and_then(Variant::as_int) else {
        return 0;
    };
    match index {
        0 | 1 if (0..=10).contains(&v) => v,
        2 | 4 if (0..=1).contains(&v) => v,
        3 if (0..4).contains(&v) => v,
        5 | 6 | 7 | 8 if (0..=255).contains(&v) => v,
        _ => 0,
    }
}

fn sanitize_config_etc_value(index: usize, arg: Option<&Variant>) -> i32 {
    let Some(v) = arg.and_then(Variant::as_int) else {
        return 0;
    };
    match index {
        0..=2 if (0..=1).contains(&v) => v,
        _ => 0,
    }
}

fn sanitize_config_sound_value(arg: Option<&Variant>) -> i32 {
    let Some(v) = arg.and_then(Variant::as_int) else {
        return 0;
    };
    if (0..=100).contains(&v) { v } else { 0 }
}


fn get_quick_slot(arg: &Variant) -> Option<i32> {
    match arg {
        Variant::Int(v) if (0..4).contains(v) => Some(*v),
        Variant::Nil => Some(0),
        _ => None,
    }
}

pub(crate) fn on_legacy_text_print(content: &str, text_id: i32) {
    let mut states = LEGACY_TEXT_STATE.lock().unwrap();
    let Some(state) = states.get_mut(&text_id) else {
        return;
    };
    if !state.enabled {
        return;
    }

    let slot0 = if state.pending_slot0.is_nil() {
        Variant::String(content.to_owned())
    } else {
        state.pending_slot0.clone()
    };
    let slot1 = state.pending_slot1.clone();

    state.history.insert(0, LegacyTextHistoryEntry { slot0, slot1 });
    if state.history.len() > 100 {
        state.history.truncate(100);
    }
    state.pending_slot0 = Variant::Nil;
    state.pending_slot1 = Variant::Nil;
}

pub fn chr_add(
    game_data: &mut GameData,
    name: &Variant,
    color_slot: &Variant,
    volume: &Variant,
    voice_prefix: &Variant,
) -> Result<Variant> {
    let Some(name) = get_string_arg(name) else {
        return Ok(Variant::Nil);
    };
    let Some(color_slot) = color_slot.as_int() else {
        return Ok(Variant::Nil);
    };
    if !(0..=255).contains(&color_slot) {
        return Ok(Variant::Nil);
    }
    let Some(volume) = volume.as_int() else {
        return Ok(Variant::Nil);
    };
    if !(0..=100).contains(&volume) {
        return Ok(Variant::Nil);
    }
    let Some(voice_prefix) = get_string_arg(voice_prefix) else {
        return Ok(Variant::Nil);
    };

    let color = game_data.motion_manager.color_manager.get_entry(color_slot as u8).clone();
    let entry = LegacyChrEntry {
        _name: name,
        _color_slot: color_slot as u8,
        rgba: [color.get_r(), color.get_g(), color.get_b(), color.get_a()],
        volume,
        _voice_prefix: normalize_prefix(&voice_prefix),
    };

    let mut table = LEGACY_CHR_TABLE.lock().unwrap();
    if table.len() < 32 {
        table.push(entry);
    }
    Ok(Variant::Nil)
}

pub fn chr_get_rgb(game_data: &mut GameData, index: &Variant, dst_slot: &Variant) -> Result<Variant> {
    let Some(index) = index.as_int() else {
        return Ok(Variant::Nil);
    };
    let Some(dst_slot) = dst_slot.as_int() else {
        return Ok(Variant::Nil);
    };
    if index < 0 || !(0..=255).contains(&dst_slot) {
        return Ok(Variant::Nil);
    }

    let table = LEGACY_CHR_TABLE.lock().unwrap();
    let Some(entry) = table.get(index as usize) else {
        return Ok(Variant::Nil);
    };

    let color = game_data.motion_manager.color_manager.get_entry_mut(dst_slot as u8);
    color.set_r(entry.rgba[0]);
    color.set_g(entry.rgba[1]);
    color.set_b(entry.rgba[2]);
    color.set_a(entry.rgba[3]);
    Ok(Variant::Nil)
}

pub fn chr_get_vol(index: &Variant) -> Result<Variant> {
    let Some(index) = index.as_int() else {
        return Ok(Variant::Nil);
    };
    if index < 0 {
        return Ok(Variant::Nil);
    }
    let table = LEGACY_CHR_TABLE.lock().unwrap();
    let Some(entry) = table.get(index as usize) else {
        return Ok(Variant::Nil);
    };
    Ok(Variant::Int(entry.volume))
}

/// AngelWish `ConfigDisplay(9)` (`sub_40A310`) validates each staged slot separately.
pub fn config_display(args: &[Variant]) -> Result<Variant> {
    let mut state = LEGACY_CONFIG_STATE.lock().unwrap();
    for (i, dst) in state.display.iter_mut().enumerate() {
        *dst = sanitize_config_display_value(i, args.get(i));
    }
    Ok(Variant::Nil)
}

/// AngelWish `ConfigEtc(3)` (`sub_40A430`) only accepts boolean-like 0/1 values.
pub fn config_etc(args: &[Variant]) -> Result<Variant> {
    let mut state = LEGACY_CONFIG_STATE.lock().unwrap();
    for (i, dst) in state.etc.iter_mut().enumerate() {
        *dst = sanitize_config_etc_value(i, args.get(i));
    }
    Ok(Variant::Nil)
}

/// AngelWish `ConfigSound(5)` (`sub_40A900`) clamps each sound-type volume to 0..100.
pub fn config_sound(args: &[Variant]) -> Result<Variant> {
    let mut state = LEGACY_CONFIG_STATE.lock().unwrap();
    for (i, dst) in state.sound.iter_mut().enumerate() {
        *dst = sanitize_config_sound_value(args.get(i));
    }
    Ok(Variant::Nil)
}

/// Original AngelWish `ConfigSet()` (`sub_40A6C0`) is a one-shot bridge from the staged
/// syscall-local slots into the engine-wide config table.
///
/// Reverse summary from the uploaded database:
/// - slots 0..8  come from `ConfigDisplay`
/// - slots 9..13 come from `ConfigSound`
/// - slots 14..16 come from `ConfigEtc`
/// - after the first successful apply, later calls return immediately
/// - the only directly-confirmed runtime side effect in the uploaded database is that
///   sound slots 9..13 are applied to active BGM/SE objects via `sub_410EA0`
pub fn config_set(game_data: &mut GameData) -> Result<Variant> {
    let sound_values = {
        let mut state = LEGACY_CONFIG_STATE.lock().unwrap();
        if state.configured {
            return Ok(Variant::Nil);
        }
        state.configured = true;
        state.sound
    };

    for (kind, vol_i) in sound_values.into_iter().enumerate() {
        let vol_i = vol_i.clamp(0, 100);
        let vol = vol_i as f32 / 100.0;
        game_data
            .bgm_player_mut()
            .set_type_volume(kind as i32, vol, kira::Tween::default());
        game_data
            .se_player_mut()
            .set_type_volume(kind as i32, vol, kira::Tween::default());
    }

    Ok(Variant::Nil)
}

/// Original LoadFile enters the engine's native load dialog. The current rfvp
/// tree does not have a slot-picker UI consumer in Rust; do not invent a new
/// pending flag here. Reuse the existing VM yield point only.
pub fn load_file(game_data: &mut GameData) -> Result<Variant> {
    set_pending_save_load_request(LegacySaveLoadRequest::LoadFile);
    game_data.thread_wrapper.should_break();
    Ok(Variant::Nil)
}


/// Original SaveFile enters the engine's native save dialog. Reuse the current
/// two-phase save path by preparing `local_saved`, then yield. Slot selection
/// remains with the existing save/load consumer layer; no new compatibility flag
/// is introduced here.
pub fn save_file(game_data: &mut GameData) -> Result<Variant> {
    game_data.save_manager.request_prepare_local_savedata();
    set_pending_save_load_request(LegacySaveLoadRequest::SaveFile);
    game_data.thread_wrapper.should_break();
    Ok(Variant::Nil)
}


/// Original LoadTitle is not the file-slot load dialog. Reverse shows it flips
/// an internal title-state mode and lets the main loop restore an already-cached
/// title snapshot. Current rfvp does not have that separate title-state cache,
/// so this compatibility entry must not open the save/load UI.
pub fn load_title(_game_data: &mut GameData) -> Result<Variant> {
    Ok(Variant::Nil)
}


/// Original SaveTitle is not the file-slot save dialog. Reverse shows it stores
/// the current state into a title-state cache and updates a native menu item.
/// Current rfvp has no separate title-state cache/menu shell, so this must not
/// open the save/load UI.
pub fn save_title(_game_data: &mut GameData) -> Result<Variant> {
    Ok(Variant::Nil)
}


pub fn movie_play_legacy(game_data: &mut GameData, path: &Variant, flag: &Variant) -> Result<Variant> {
    movie_play(game_data, path, flag)
}

/// Original PrimSetClip resets the prim clip rect to the full virtual screen. rfvp does not
/// carry a separate legacy clip rectangle on Prim, so we map this to the current UV/WH rect
/// representation, which is effect-equivalent for the existing renderer.
pub fn prim_set_clip_legacy(game_data: &mut GameData, prim_id: &Variant) -> Result<Variant> {
    let Some(prim_id) = prim_id.as_int() else {
        return Ok(Variant::Nil);
    };
    if !(1..=1023).contains(&prim_id) {
        return Ok(Variant::Nil);
    }

    let sw = game_data.get_width() as i32;
    let sh = game_data.get_height() as i32;
    game_data.motion_manager.prim_manager.prim_set_uv(prim_id, 0, 0);
    game_data.motion_manager.prim_manager.prim_set_size(prim_id, sw, sh);
    let mut prim = game_data.motion_manager.prim_manager.get_prim(prim_id as i16);
    prim.apply_attr(0x40);
    Ok(Variant::Nil)
}

pub fn quick_copy(game_data: &mut GameData, src: &Variant, dst: &Variant) -> Result<Variant> {
    let Some(src) = src.as_int() else {
        return Ok(Variant::Nil);
    };
    let Some(dst) = dst.as_int() else {
        return Ok(Variant::Nil);
    };
    if !(0..4).contains(&src) || !(0..4).contains(&dst) || src == dst {
        return Ok(Variant::Nil);
    }
    let nls = game_data.get_nls();
    if game_data.save_manager.load_savedata(src as u32, nls).is_ok() {
        let _ = game_data.save_manager.copy_savedata(src as u32, dst as u32);
    }
    Ok(Variant::Nil)
}

pub fn quick_state(game_data: &mut GameData, slot: &Variant) -> Result<Variant> {
    let Some(slot) = get_quick_slot(slot) else {
        return Ok(Variant::Nil);
    };
    if game_data.save_manager.test_save_slot(slot as u32) {
        Ok(Variant::True)
    } else {
        Ok(Variant::Nil)
    }
}

pub fn load_quick(game_data: &mut GameData, slot: &Variant) -> Result<Variant> {
    let Some(slot) = get_quick_slot(slot) else {
        return Ok(Variant::Nil);
    };
    load(game_data, &Variant::Int(slot))
}

pub fn save_quick(game_data: &mut GameData, slot: &Variant) -> Result<Variant> {
    let Some(slot) = get_quick_slot(slot) else {
        return Ok(Variant::Nil);
    };
    save_write(game_data, &Variant::Int(slot))
}

/// SaveLoadMenu only toggled the old native menu shell around the title flow.
/// It must not open the file-slot save/load UI in rfvp.
pub fn save_load_menu(_game_data: &mut GameData, _flag: &Variant) -> Result<Variant> {
    Ok(Variant::Nil)
}


/// SaveName writes the two strings that the original engine later serializes into save metadata.
/// We therefore mirror it directly onto rfvp's SaveManager current title fields.
pub fn save_name(game_data: &mut GameData, left: &Variant, right: &Variant) -> Result<Variant> {
    let left_s = get_string_arg(left);
    let right_s = get_string_arg(right);

    {
        let mut state = LEGACY_UI_STATE.lock().unwrap();
        state.save_name_left = left_s.clone();
        state.save_name_right = right_s.clone();
    }

    if let Some(s) = left_s {
        game_data.save_manager.set_current_title(s);
    }
    if let Some(s) = right_s {
        game_data.save_manager.set_current_scene_title(s);
    }
    Ok(Variant::Nil)
}

pub fn sound_pan(game_data: &mut GameData, channel: &Variant, pan: &Variant) -> Result<Variant> {
    let Some(channel) = channel.as_int() else {
        return Ok(Variant::Nil);
    };
    let Some(pan) = pan.as_int() else {
        return Ok(Variant::Nil);
    };
    if !(0..32).contains(&channel) || !(-100..=100).contains(&pan) {
        return Ok(Variant::Nil);
    }
    let normalized = (pan as f64 + 100.0) / 200.0;
    game_data.se_player_mut().set_panning(channel, normalized, kira::Tween::default());
    Ok(Variant::Nil)
}

pub fn text_data_set(text_id: &Variant, key: &Variant, value: &Variant) -> Result<Variant> {
    let Some(text_id) = text_id.as_int() else {
        return Ok(Variant::Nil);
    };
    if !(0..8).contains(&text_id) {
        return Ok(Variant::Nil);
    }
    let Some(key) = key.as_int() else {
        return Ok(Variant::Nil);
    };

    let mut states = LEGACY_TEXT_STATE.lock().unwrap();
    let state = states.entry(text_id).or_default();
    match key {
        0 => state.pending_slot0 = value.clone(),
        1 => state.pending_slot1 = value.clone(),
        _ => {}
    }
    Ok(Variant::Nil)
}

pub fn text_data_get(text_id: &Variant, index: &Variant, key: &Variant) -> Result<Variant> {
    let Some(text_id) = text_id.as_int() else {
        return Ok(Variant::Nil);
    };
    let Some(index) = index.as_int() else {
        return Ok(Variant::Nil);
    };
    let Some(key) = key.as_int() else {
        return Ok(Variant::Nil);
    };
    if !(0..8).contains(&text_id) || index < 0 {
        return Ok(Variant::Nil);
    }

    let states = LEGACY_TEXT_STATE.lock().unwrap();
    let Some(state) = states.get(&text_id) else {
        return Ok(Variant::Nil);
    };
    let Some(entry) = state.history.get(index as usize) else {
        return Ok(Variant::Nil);
    };

    let out = match key {
        0 => entry.slot0.clone(),
        1 => entry.slot1.clone(),
        _ => Variant::Nil,
    };
    Ok(out)
}

pub fn text_history(text_id: &Variant, mode: &Variant, prim_a: &Variant, prim_b: &Variant) -> Result<Variant> {
    let Some(text_id) = text_id.as_int() else {
        return Ok(Variant::Nil);
    };
    if !(0..8).contains(&text_id) {
        return Ok(Variant::Nil);
    }

    let mut states = LEGACY_TEXT_STATE.lock().unwrap();
    let state = states.entry(text_id).or_default();

    if let Some(v) = mode.as_int() {
        if v == -1 {
            return Ok(Variant::Int(state.history.len() as i32));
        }
        if v == 0 {
            state.enabled = false;
            return Ok(Variant::Nil);
        }
        if v > 0 {
            return Ok(state.history.get(v as usize).map(|e| e.slot0.clone()).unwrap_or(Variant::Nil));
        }
    }

    if mode.canbe_true() && !mode.is_nil() {
        state.enabled = true;
        state.prim_a = prim_a.as_int();
        state.prim_b = prim_b.as_int();
    }

    Ok(Variant::Nil)
}

pub fn text_hyphenation(game_data: &mut GameData, text_id: &Variant, _limit: &Variant, chars: &Variant) -> Result<Variant> {
    text_suspend_chr(game_data, text_id, chars)
}

macro_rules! simple_syscaller {
    ($name:ident, $body:expr) => {
        pub struct $name;
        impl Syscaller for $name {
            fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
                $body(game_data, args)
            }
        }
        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}
    };
}

simple_syscaller!(ChrAdd, |game_data: &mut GameData, args: Vec<Variant>| chr_add(game_data, get_var!(args, 0), get_var!(args, 1), get_var!(args, 2), get_var!(args, 3)));
simple_syscaller!(ChrGetRGB, |game_data: &mut GameData, args: Vec<Variant>| chr_get_rgb(game_data, get_var!(args, 0), get_var!(args, 1)));
simple_syscaller!(ChrGetVol, |_game_data: &mut GameData, args: Vec<Variant>| chr_get_vol(get_var!(args, 0)));
simple_syscaller!(ConfigDisplay, |_game_data: &mut GameData, args: Vec<Variant>| config_display(&args));
simple_syscaller!(ConfigEtc, |_game_data: &mut GameData, args: Vec<Variant>| config_etc(&args));
simple_syscaller!(ConfigSet, |game_data: &mut GameData, _args: Vec<Variant>| config_set(game_data));
simple_syscaller!(ConfigSound, |_game_data: &mut GameData, args: Vec<Variant>| config_sound(&args));
simple_syscaller!(LoadFile, |game_data: &mut GameData, _args: Vec<Variant>| load_file(game_data));
simple_syscaller!(LoadQuick, |game_data: &mut GameData, args: Vec<Variant>| load_quick(game_data, get_var!(args, 0)));
simple_syscaller!(LoadTitle, |game_data: &mut GameData, _args: Vec<Variant>| load_title(game_data));
simple_syscaller!(MoviePlay, |game_data: &mut GameData, args: Vec<Variant>| movie_play_legacy(game_data, get_var!(args, 0), get_var!(args, 1)));
simple_syscaller!(PrimSetClip, |game_data: &mut GameData, args: Vec<Variant>| prim_set_clip_legacy(game_data, get_var!(args, 0)));
simple_syscaller!(QuickCopy, |game_data: &mut GameData, args: Vec<Variant>| quick_copy(game_data, get_var!(args, 0), get_var!(args, 1)));
simple_syscaller!(QuickState, |game_data: &mut GameData, args: Vec<Variant>| quick_state(game_data, get_var!(args, 0)));
simple_syscaller!(SaveFile, |game_data: &mut GameData, _args: Vec<Variant>| save_file(game_data));
simple_syscaller!(SaveLoadMenu, |game_data: &mut GameData, args: Vec<Variant>| save_load_menu(game_data, get_var!(args, 0)));
simple_syscaller!(SaveName, |game_data: &mut GameData, args: Vec<Variant>| save_name(game_data, get_var!(args, 0), get_var!(args, 1)));
simple_syscaller!(SaveQuick, |game_data: &mut GameData, args: Vec<Variant>| save_quick(game_data, get_var!(args, 0)));
simple_syscaller!(SaveTitle, |game_data: &mut GameData, _args: Vec<Variant>| save_title(game_data));
simple_syscaller!(SoundPan, |game_data: &mut GameData, args: Vec<Variant>| sound_pan(game_data, get_var!(args, 0), get_var!(args, 1)));
simple_syscaller!(TextDataGet, |_game_data: &mut GameData, args: Vec<Variant>| text_data_get(get_var!(args, 0), get_var!(args, 1), get_var!(args, 2)));
simple_syscaller!(TextDataSet, |_game_data: &mut GameData, args: Vec<Variant>| text_data_set(get_var!(args, 0), get_var!(args, 1), get_var!(args, 2)));
simple_syscaller!(TextHistory, |_game_data: &mut GameData, args: Vec<Variant>| text_history(get_var!(args, 0), get_var!(args, 1), get_var!(args, 2), get_var!(args, 3)));
simple_syscaller!(TextHyphenation, |game_data: &mut GameData, args: Vec<Variant>| text_hyphenation(game_data, get_var!(args, 0), get_var!(args, 1), get_var!(args, 2)));
