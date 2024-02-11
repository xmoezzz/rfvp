use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::resources::audio::{PlayConfig, Sound};
use crate::subsystem::world::GameData;

/// load audio on a specific channel, used for voice and sound effects
pub fn audio_load(game_data: &mut GameData, channel: i32, path: &Variant) -> Result<Variant> {
    if !(0..4).contains(&channel) {
        log::error!("audio_play: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let path = match path {
        Variant::String(path) => {
            path.clone()
        }
        // unload channel
        Variant::Nil => {
            game_data.audio().stop_audio(channel as usize)?;
            return Ok(Variant::Nil);
        }
        _ => {
            log::error!("audio_load: Invalid path {:?}", path);
            return Ok(Variant::Nil);
        }
    };

    let buffer = game_data.vfs_load_file(&path)?;
    let config = PlayConfig {
        volume: 1.0,
        looped: false,
        category: Sound::SoundEffect,
        path: path.to_string(),
        crossfade: 0,
    };
    
    if let Err(e) = game_data.audio().load_audio(buffer, channel as usize, config) {
        log::error!("audio_load: {:?}", e);
    }

    Ok(Variant::Nil)
}

/// play audio on a specific channel, and loop it if needed
pub fn audio_play(game_data: &mut GameData, channel: i32, looped: bool) -> Result<Variant> {
    if !(0..4).contains(&channel) {
        log::error!("audio_play: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    if let Err(e) = game_data.audio().play_audio(channel as usize, looped) {
        log::error!("audio_play: {:?}", e);
    }

    Ok(Variant::Nil)
}

/// stop audio on a specific channel
pub fn audio_stop(game_data: &mut GameData, channel: i32, _volume: &Variant) -> Result<Variant> {
    if !(0..4).contains(&channel) {
        log::error!("audio_stop: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    game_data.audio().stop_audio(channel as usize)?;

    Ok(Variant::Nil)
}


/// pause audio on a specific channel
pub fn audio_pause(game_data: &mut GameData, channel: i32) -> Result<Variant> {
    if !(0..4).contains(&channel) {
        log::error!("audio_pause: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    game_data.audio().pause_audio(channel as usize)?;

    Ok(Variant::Nil)
}

/// test the specific channel is playing or not
pub fn audio_state(game_data: &mut GameData, channel: i32) -> Result<Variant> {
    if !(0..4).contains(&channel) {
        log::error!("audio_state: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    match game_data.audio().audio_is_playing(channel as usize) {
        true => Ok(Variant::True),
        false => Ok(Variant::Nil),
    }
}

pub fn audio_set_type(game_data: &mut GameData, channel: i32, sound_type: i32) -> Result<Variant> {
    if !(0..4).contains(&channel) {
        log::error!("audio_set_type: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    if !(0..10).contains(&sound_type) {
        log::error!("audio_set_type: Invalid sound type {}", sound_type);
        return Ok(Variant::Nil);
    }

    game_data.audio().audio_set_type(channel as usize, sound_type);

    Ok(Variant::Nil)
}

// set the volume of the specific channel
pub fn audio_set_volume(game_data: &mut GameData, channel: i32, volume: &Variant) -> Result<Variant> {
    if !(0..4).contains(&channel) {
        log::error!("audio_set_volume: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    if !volume.is_int() && !volume.is_nil() {
        log::error!("audio_set_volume: Invalid volume {:?}", volume);
        return Ok(Variant::Nil);
    };

    // TODO:
    // sub_436080
    // in the original implementation, the volume should be sound power, then convert to db
    // to call IDirectSoundBuffer::SetVolume
    let volume = if volume.is_nil() {
        0.0
    } else {
        volume.as_int().unwrap() as f32 / 30000.0
    };

    game_data.audio().audio_set_volume(channel as usize, volume);

    Ok(Variant::Nil)
}


/// load sound on a specific channel, used for voice and sound effects
pub fn sound_load(game_data: &mut GameData, channel: i32, path: &Variant) -> Result<Variant> {
    if !(0..256).contains(&channel) {
        log::error!("sound_play: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    let path = match path {
        Variant::String(path) => {
            path.clone()
        }
        // unload channel
        Variant::Nil => {
            game_data.audio().stop_sound(channel as usize)?;
            return Ok(Variant::Nil);
        }
        _ => {
            log::error!("audio_load: Invalid path {:?}", path);
            return Ok(Variant::Nil);
        }
    };

    let buffer = game_data.vfs_load_file(&path)?;
    let config = PlayConfig {
        volume: 1.0,
        looped: false,
        category: Sound::SoundEffect,
        path: path.to_string(),
        crossfade: 0,
    };
    
    if let Err(e) = game_data.audio().sound_load(buffer, channel as usize, config) {
        log::error!("audio_load: {:?}", e);
    }

    Ok(Variant::Nil)
}


pub fn sound_master_volume(game_data: &mut GameData, volume: i32) -> Result<Variant> {
    if !(0..=100).contains(&volume) {
        log::error!("sound_master_volume: Invalid volume {}", volume);
        return Ok(Variant::Nil);
    }

    game_data.audio().set_master_volume(volume as f32 / 100.0);
    Ok(Variant::Nil)
}

pub fn sound_play(game_data: &mut GameData, channel: i32, looped: bool, volume: &Variant) -> Result<Variant> {
    if !(0..256).contains(&channel) {
        log::error!("sound_play: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    if !volume.is_int() && !volume.is_nil() {
        log::error!("audio_set_volume: Invalid volume {:?}", volume);
        return Ok(Variant::Nil);
    };

    let volume = if volume.is_nil() {
        0.0
    } else {
        volume.as_int().unwrap() as f32 / 30000.0
    };

    if let Err(e) = game_data.audio().play_sound(channel as usize, looped, volume) {
        log::error!("sound_play: {:?}", e);
    }

    Ok(Variant::Nil)
}

pub fn sound_stop(game_data: &mut GameData, channel: i32, _volume: &Variant) -> Result<Variant> {
    if !(0..256).contains(&channel) {
        log::error!("sound_stop: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    game_data.audio().stop_sound(channel as usize)?;

    Ok(Variant::Nil)
}

pub fn sound_type(game_data: &mut GameData, channel: i32, sound_type: i32) -> Result<Variant> {
    if !(0..256).contains(&channel) {
        log::error!("sound_type: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    if !(0..10).contains(&sound_type) {
        log::error!("sound_type: Invalid sound type {}", sound_type);
        return Ok(Variant::Nil);
    }

    game_data.audio().sound_set_type(channel as usize, sound_type);

    Ok(Variant::Nil)
}

pub fn sound_type_volume(game_data: &mut GameData, sound_type: i32, volume: i32) -> Result<Variant> {
    if !(0..10).contains(&sound_type) {
        log::error!("sound_type_volume: Invalid sound type {}", sound_type);
        return Ok(Variant::Nil);
    }

    if !(0..=100).contains(&volume) {
        log::error!("sound_type_volume: Invalid volume {}", volume);
        return Ok(Variant::Nil);
    }

    game_data.audio().sound_set_type_volume(sound_type, volume as f32 / 100.0);

    Ok(Variant::Nil)
}

pub fn sound_volume(game_data: &mut GameData, channel: i32, volume: i32, volume2: &Variant) -> Result<Variant> {
    if !(0..256).contains(&channel) {
        log::error!("sound_volume: Invalid channel {}", channel);
        return Ok(Variant::Nil);
    }

    if !(0..=100).contains(&volume) {
        log::error!("sound_volume: Invalid volume {}", volume);
        return Ok(Variant::Nil);
    }

    let volume = volume as f32 / 100.0;

    if !volume2.is_int() && !volume2.is_nil() {
        log::error!("sound_volume: Invalid volume {:?}", volume);
        return Ok(Variant::Nil);
    };

    let volume2 = if volume2.is_nil() {
        0.0
    } else {
        volume2.as_int().unwrap() as f32 / 30000.0
    };

    game_data.audio().sound_set_volume(channel as usize, volume);

    Ok(Variant::Nil)
}

