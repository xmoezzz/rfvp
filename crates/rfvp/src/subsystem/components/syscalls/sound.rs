use anyhow::Result;
use bevy_utils::Duration;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::Syscaller;

/// load audio on a specific channel, used for voice and sound effects
pub fn audio_load(game_data: &mut GameData, channel: &Variant, path: &Variant) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("audio_load: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..4).contains(&channel) {
        log::error!("audio_play: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    match path {
        Variant::String(path) | Variant::ConstString(path, _) => {
            let path = path.clone();
            let data = game_data.vfs_load_file(&path)?;
            if let Err(e) = game_data.bgm_player_mut().load_named(channel, path.clone(), data) {
                log::error!("audio_load: {:?}", e);
            }
            return Ok(Variant::Nil);
        },
        // unload channel
        Variant::Nil => {
            game_data.bgm_player_mut().stop(channel, kira::Tween::default());
            return Ok(Variant::Nil);
        }
        _ => {
            log::error!("audio_load: Invalid path {:?}", path);
            return Ok(Variant::Nil);
        }
    };
}

/// play audio on a specific channel, and loop it if needed
pub fn audio_play(
    game_data: &mut GameData,
    channel: &Variant,
    looped: &Variant,
) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("audio_play: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..4).contains(&channel) {
        log::error!("audio_play: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let looped = looped.canbe_true();

    if let Err(e) = game_data.bgm_player_mut().play(
        channel,
        looped,
        1.0 as f32,
        0.5,
        kira::Tween::default(),
    ) {
        log::error!("audio_play: {:?}", e);
    }

    Ok(Variant::Nil)
}

/// stop audio on a specific channel
pub fn audio_stop(
    game_data: &mut GameData,
    channel: &Variant,
    fadeout: &Variant,
) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("audio_stop: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..4).contains(&channel) {
        log::error!("audio_stop: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let mut fadeout = fadeout.as_int().unwrap_or(0);
    if !(0..=300000).contains(&fadeout) {
        fadeout = 0;
    }

    let fade_out = kira::Tween {
        duration: Duration::from_millis(fadeout as u64),
        ..Default::default()
    };

    game_data.bgm_player_mut().stop(channel, fade_out);

    Ok(Variant::Nil)
}

/// Silence audio on a specific channel (BGM).
///
/// This implements the engine's `AudioSilentOn` semantics: once enabled, the channel remains muted.
pub fn audio_silent_on(game_data: &mut GameData, channel: &Variant) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("audio_silent_on: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..4).contains(&channel) {
        log::error!("audio_silent_on: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    game_data.bgm_player_mut().silent_on(channel, kira::Tween::default());
    Ok(Variant::Nil)
}

/// Backward-compatible misspelling.
pub fn audio_slient_on(game_data: &mut GameData, channel: &Variant) -> Result<Variant> {
    audio_silent_on(game_data, channel)
}

/// test the specific channel is playing or not
pub fn audio_state(game_data: &mut GameData, channel: &Variant) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("audio_state: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..4).contains(&channel) {
        log::error!("audio_state: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    match game_data.bgm_player_mut().is_playing(channel) {
        true => Ok(Variant::True),
        false => Ok(Variant::Nil),
    }
}

// asscociate the sound type with the specific channel
pub fn audio_type(
    game_data: &mut GameData,
    channel: &Variant,
    sound_type: &Variant,
) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("audio_type: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..4).contains(&channel) {
        log::error!("audio_set_type: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let sound_type = match sound_type {
        Variant::Int(sound_type) => *sound_type,
        _ => {
            log::error!("audio_type: Invalid sound type {:?}", sound_type);
            return Ok(Variant::Nil);
        }
    };

    if !(0..10).contains(&sound_type) {
        log::error!("audio_type: Invalid sound type {}", sound_type);
        return Ok(Variant::Nil);
    }

    game_data.bgm_player_mut().set_type(channel, sound_type);

    Ok(Variant::Nil)
}

// set the volume of the specific channel
pub fn audio_vol(
    game_data: &mut GameData,
    channel: &Variant,
    volume: &Variant,
    crossfade: &Variant,
) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("audio_vol: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..4).contains(&channel) {
        log::error!("audio_set_volume: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let volume = match volume {
        Variant::Int(volume) => *volume,
        _ => {
            log::error!("audio_vol: Invalid volume {:?}", volume);
            return Ok(Variant::Nil);
        }
    };

    if !(0..=100).contains(&volume) {
        log::error!("audio_vol: Invalid volume {}", volume);
        return Ok(Variant::Nil);
    }

    let volume = volume as f64 / 100.0;

    let mut crossfade = crossfade.as_int().unwrap_or(0);
    if !(0..=300000).contains(&crossfade) {
        crossfade = 0;
    }

    let cross_fade = kira::Tween {
        duration: Duration::from_millis(crossfade as u64),
        ..Default::default()
    };

    game_data
        .bgm_player_mut()
        .set_volume(channel, volume as f32, cross_fade);

    Ok(Variant::Nil)
}

/// load sound on a specific channel, used for voice and sound effects
pub fn sound_load(game_data: &mut GameData, channel: &Variant, path: &Variant) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("sound_load: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..256).contains(&channel) {
        log::error!("sound_play: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    match path {
        Variant::String(path) | Variant::ConstString(path, _) => {
            let path = path.clone();
            let data = game_data.vfs_load_file(&path)?;
            if let Err(e) = game_data.se_player_mut().load_named(channel, path.clone(), data) {
                log::error!("sound_load: {:?}", e);
            }
            return Ok(Variant::Nil);
        },
        // unload channel
        Variant::Nil => {
            game_data.se_player_mut().stop(channel, kira::Tween::default());
            return Ok(Variant::Nil);
        }
        _ => {
            log::error!("sound_load: Invalid path {:?}", path);
            return Ok(Variant::Nil);
        }
    };
}

pub fn sound_master_vol(game_data: &mut GameData, volume: &Variant) -> Result<Variant> {
    let volume = match volume {
        Variant::Int(volume) => *volume,
        _ => {
            log::error!("sound_master_vol: Invalid volume {:?}", volume);
            return Ok(Variant::Nil);
        }
    };

    if !(0..=100).contains(&volume) {
        log::error!("sound_master_vol: Invalid volume {}", volume);
        return Ok(Variant::Nil);
    }

    game_data.audio_manager().master_vol(volume as f32 / 100.0);
    Ok(Variant::Nil)
}

pub fn sound_play(
    game_data: &mut GameData,
    channel: &Variant,
    looped: &Variant,
    fadein: &Variant,
) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("sound_play: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..256).contains(&channel) {
        log::error!("sound_play: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let mut fadein = fadein.as_int().unwrap_or(0);
    if !(0..=300000).contains(&fadein) {
        fadein = 0;
    }

    let looped = looped.canbe_true();
    let fade_in = kira::Tween {
        duration: core::time::Duration::from_millis(fadein as u64),
        ..Default::default()
    };

    if let Err(e) =
        game_data
            .se_player_mut()
            .play(channel, looped, 1.0, 0.5, fade_in)
    {
        log::error!("sound_play: {:?}", e);
    }

    Ok(Variant::Nil)
}

/// Silence sound on a specific channel (SE).
///
/// This implements the engine's `SoundSilentOn` semantics: once enabled, the channel remains muted.
pub fn sound_silent_on(game_data: &mut GameData, channel: &Variant) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("sound_silent_on: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..256).contains(&channel) {
        log::error!("sound_silent_on: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    game_data.se_player_mut().silent_on(channel, kira::Tween::default());
    Ok(Variant::Nil)
}

/// Backward-compatible misspelling.
pub fn sound_slient_on(game_data: &mut GameData, channel: &Variant) -> Result<Variant> {
    sound_silent_on(game_data, channel)
}

// stop sound on a specific channel with fadeout
pub fn sound_stop(
    game_data: &mut GameData,
    channel: &Variant,
    fadeout: &Variant,
) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("sound_stop: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..256).contains(&channel) {
        log::error!("sound_stop: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let mut fadeout = fadeout.as_int().unwrap_or(0);
    if !(0..=300000).contains(&fadeout) {
        fadeout = 0;
    }

    let fade_out = kira::Tween {
        duration: Duration::from_millis(fadeout as u64),
        ..Default::default()
    };

    game_data.se_player_mut().stop(channel, fade_out);

    Ok(Variant::Nil)
}

// asscociate the sound type with the specific channel
pub fn sound_type(
    game_data: &mut GameData,
    channel: &Variant,
    sound_type: &Variant,
) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("sound_type: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..256).contains(&channel) {
        log::error!("sound_type: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let sound_type = match sound_type {
        Variant::Int(sound_type) => *sound_type,
        _ => {
            log::error!("sound_type: Invalid sound type {:?}", sound_type);
            return Ok(Variant::Nil);
        }
    };

    if !(0..10).contains(&sound_type) {
        log::error!("sound_type: Invalid sound type {}", sound_type);
        return Ok(Variant::Nil);
    }

    game_data.se_player_mut().set_type(channel, sound_type);

    Ok(Variant::Nil)
}

pub fn sound_type_vol(
    game_data: &mut GameData,
    sound_type: &Variant,
    volume: &Variant,
) -> Result<Variant> {
    let sound_type = match sound_type {
        Variant::Int(sound_type) => *sound_type,
        _ => {
            log::error!("sound_type_volume: Invalid sound type {:?}", sound_type);
            return Ok(Variant::Nil);
        }
    };

    if !(0..10).contains(&sound_type) {
        log::error!("sound_type_volume: Invalid sound type {}", sound_type);
        return Ok(Variant::Nil);
    }

    let volume = match volume {
        Variant::Int(volume) => *volume,
        _ => {
            log::error!("sound_type_volume: Invalid volume {:?}", volume);
            return Ok(Variant::Nil);
        }
    };

    if !(0..=100).contains(&volume) {
        log::error!("sound_type_volume: Invalid volume {}", volume);
        return Ok(Variant::Nil);
    }

    game_data.se_player_mut().set_type_volume(
        sound_type,
        volume as f32 / 100.0,
        kira::Tween::default(),
    );

    Ok(Variant::Nil)
}

pub fn sound_volume(
    game_data: &mut GameData,
    channel: &Variant,
    volume: &Variant,
    crossfade: &Variant,
) -> Result<Variant> {
    let channel = match channel {
        Variant::Int(channel) => *channel,
        _ => {
            log::error!("sound_volume: Invalid channel {:?}", channel);
            return Ok(Variant::Nil);
        }
    };

    if !(0..256).contains(&channel) {
        log::error!("sound_volume: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let volume = match volume {
        Variant::Int(volume) => *volume,
        _ => {
            log::error!("sound_volume: Invalid volume {:?}", volume);
            return Ok(Variant::Nil);
        }
    };

    if !(0..=100).contains(&volume) {
        log::error!("sound_volume: Invalid volume {}", volume);
        return Ok(Variant::Nil);
    }

    let volume = volume as f64 / 100.0;

    let mut crossfade = crossfade.as_int().unwrap_or(0);
    if !(0..=300000).contains(&crossfade) {
        crossfade = 0;
    }

    let cross_fade = kira::Tween {
        duration: Duration::from_millis(crossfade as u64),
        ..Default::default()
    };

    game_data
        .se_player_mut()
        .set_volume(channel, volume as f32, cross_fade);

    Ok(Variant::Nil)
}

pub struct AudioLoad;
impl Syscaller for AudioLoad {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        audio_load(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for AudioLoad {}
unsafe impl Sync for AudioLoad {}

pub struct AudioPlay;
impl Syscaller for AudioPlay {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        audio_play(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for AudioPlay {}
unsafe impl Sync for AudioPlay {}

pub struct AudioSilentOn;
impl Syscaller for AudioSilentOn {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        audio_silent_on(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for AudioSilentOn {}
unsafe impl Sync for AudioSilentOn {}

pub struct AudioSlientOn;
impl Syscaller for AudioSlientOn {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        audio_slient_on(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for AudioSlientOn {}
unsafe impl Sync for AudioSlientOn {}

pub struct AudioState;
impl Syscaller for AudioState {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        audio_state(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for AudioState {}
unsafe impl Sync for AudioState {}

pub struct AudioStop;
impl Syscaller for AudioStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        audio_stop(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for AudioStop {}
unsafe impl Sync for AudioStop {}

pub struct AudioType;
impl Syscaller for AudioType {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        audio_type(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for AudioType {}
unsafe impl Sync for AudioType {}

pub struct AudioVol;
impl Syscaller for AudioVol {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        audio_vol(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for AudioVol {}
unsafe impl Sync for AudioVol {}

pub struct SoundLoad;
impl Syscaller for SoundLoad {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_load(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for SoundLoad {}
unsafe impl Sync for SoundLoad {}

pub struct SoundMasterVol;
impl Syscaller for SoundMasterVol {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_master_vol(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for SoundMasterVol {}
unsafe impl Sync for SoundMasterVol {}

pub struct SoundPlay;
impl Syscaller for SoundPlay {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_play(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for SoundPlay {}
unsafe impl Sync for SoundPlay {}

pub struct SoundSilentOn;
impl Syscaller for SoundSilentOn {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_silent_on(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for SoundSilentOn {}
unsafe impl Sync for SoundSilentOn {}

pub struct SoundSlientOn;
impl Syscaller for SoundSlientOn {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_slient_on(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for SoundSlientOn {}
unsafe impl Sync for SoundSlientOn {}

pub struct SoundStop;
impl Syscaller for SoundStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_stop(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for SoundStop {}
unsafe impl Sync for SoundStop {}

pub struct SoundType;
impl Syscaller for SoundType {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_type(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for SoundType {}
unsafe impl Sync for SoundType {}

pub struct SoundTypeVol;
impl Syscaller for SoundTypeVol {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_type_vol(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for SoundTypeVol {}
unsafe impl Sync for SoundTypeVol {}

pub struct SoundVol;
impl Syscaller for SoundVol {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_volume(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for SoundVol {}
unsafe impl Sync for SoundVol {}

pub struct SoundVolume;
impl Syscaller for SoundVolume {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        sound_volume(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
            super::get_var!(args, 2),
        )
    }
}

unsafe impl Send for SoundVolume {}
unsafe impl Sync for SoundVolume {}
