use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HistoryEntry {
    /// character name - 0
    pub name: Option<String>,
    /// main text - 1
    pub content: Option<String>,
    /// voice id - 3
    pub voice_id: Option<i32>,
}

pub enum HistoryFunction {
    /// for name
    Name = 0,
    /// for text content
    Content = 1,
    /// for voice
    Voice = 2,
    // when variable set to nil, push the current history into array
    // Push,
}

impl TryFrom<i32> for HistoryFunction {
    type Error = ();

    fn try_from(v: i32) -> core::result::Result<Self, Self::Error> {
        match v {
            x if x == HistoryFunction::Name as i32 => Ok(HistoryFunction::Name),
            x if x == HistoryFunction::Content as i32 => Ok(HistoryFunction::Content),
            x if x == HistoryFunction::Voice as i32 => Ok(HistoryFunction::Voice),
            _ => Err(()),
        }
    }
}


#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HistoryManager {
    histories: Vec<HistoryEntry>,
    current: HistoryEntry,
}

impl HistoryManager {
    pub fn set_name(&mut self, name: String) {
        self.current.name = Some(name);
    }

    pub fn set_content(&mut self, content: String) {
        self.current.content = Some(content);
    }

    pub fn set_voice(&mut self, voice_id: i32) {
        self.current.voice_id = Some(voice_id);
    }

    pub fn push(&mut self) {
        self.histories.insert(0, self.current.clone());
        self.current = Default::default();
    }

    pub fn get_name(&mut self, id: u32) -> Option<String> {
        self.histories
            .get(id as usize)
            .and_then(|h| h.name.clone())
    }

    pub fn get_content(&mut self, id: u32) -> Option<String> {
        self.histories
            .get(id as usize)
            .and_then(|h| h.content.clone())
    }

    pub fn get_voice(&mut self, id: u32) -> Option<i32> {
        self.histories
            .get(id as usize)
            .and_then(|h| h.voice_id)
    }
}
