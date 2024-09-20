pub mod audio;
pub mod layers;

use layers::LayersState;
use rfvp_core::{format::save::PersistData, vm::command::types::MessageboxStyle};

use crate::adv::vm_state::audio::AudioState;

pub struct SaveInfo {
    pub info: [String; 4],
}

impl SaveInfo {
    pub fn set_save_info(&mut self, level: i32, info: String) {
        assert!(
            (0..=4).contains(&level),
            "SaveInfo::set_save_info: level out of range"
        );

        self.info[level as usize] = info;
    }
}

#[derive(Debug)]
pub struct MessageState {
    pub msginit: MessageboxStyle,
    pub messagebox_shown: bool,
    pub text: Option<String>,
}

impl MessageState {
    pub fn new() -> Self {
        Self {
            msginit: MessageboxStyle::default(),
            messagebox_shown: false,
            text: None,
        }
    }
}

pub struct VmState {
    pub save_info: SaveInfo,
    pub messagebox_state: MessageState,
    pub persist: PersistData,
    pub layers: LayersState,
    pub audio: AudioState,
}

impl VmState {
    pub fn new() -> Self {
        Self {
            save_info: SaveInfo {
                info: ["", "", "", ""].map(|v| v.to_string()),
            },
            messagebox_state: MessageState::new(),
            persist: PersistData::new(),
            layers: LayersState::new(),
            audio: AudioState::new(),
        }
    }
}
