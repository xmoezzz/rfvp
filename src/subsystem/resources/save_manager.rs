use anyhow::{bail, Result};
use chrono::{Datelike, Local, Timelike};
use std::mem::size_of;
use std::{io::Read, path::Path};

use crate::{script::parser::Nls, utils::file::app_base_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDataFunction {
    LoadSaveThumbToTexture = 0,
    TestSaveData = 1,
    DeleteSaveData = 2,
    CopySaveData = 3,
    GetSaveTitle = 4,
    GetSaveSceneTitle = 5,
    GetScriptContent = 6,
    GetYear = 7,
    GetMonth = 8,
    GetDay = 9,
    GetDayOfWeek = 10,
    GetHour = 11,
    GetMinute = 12,
}

impl TryFrom<i32> for SaveDataFunction {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> anyhow::Result<Self> {
        match value {
            0 => Ok(SaveDataFunction::LoadSaveThumbToTexture),
            1 => Ok(SaveDataFunction::TestSaveData),
            2 => Ok(SaveDataFunction::DeleteSaveData),
            3 => Ok(SaveDataFunction::CopySaveData),
            4 => Ok(SaveDataFunction::GetSaveTitle),
            5 => Ok(SaveDataFunction::GetSaveSceneTitle),
            6 => Ok(SaveDataFunction::GetScriptContent),
            7 => Ok(SaveDataFunction::GetYear),
            8 => Ok(SaveDataFunction::GetMonth),
            9 => Ok(SaveDataFunction::GetDay),
            10 => Ok(SaveDataFunction::GetDayOfWeek),
            11 => Ok(SaveDataFunction::GetHour),
            12 => Ok(SaveDataFunction::GetMinute),
            _ => bail!("invalid save data function: {}", value),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SaveItem {
    year: u16,
    month: u8,
    day: u8,
    day_of_week: u8,
    hour: u8,
    minute: u8,
    title: String,
    scene_title: String,
    script_content: String,
}

impl SaveItem {
    pub fn new() -> Self {
        let local_time = Local::now();
        SaveItem {
            year: local_time.year() as u16,
            month: local_time.month() as u8,
            day: local_time.day() as u8,
            day_of_week: local_time.weekday().num_days_from_monday() as u8,
            hour: local_time.hour() as u8,
            minute: local_time.minute() as u8,
            title: String::new(),
            scene_title: String::new(),
            script_content: String::new(),
        }
    }

    pub fn read_thumb_texture(slot: u32, width: u32, height: u32) -> Result<Vec<u8>> {
        let thumb_path = app_base_path()
            .get_path()
            .join("save")
            .join(format!("s{}.bin", slot));

        let mut file = std::fs::File::open(thumb_path)?;
        let mut buf = vec![];
        file.read_to_end(&mut buf)?;

        if buf.len() < Self::calculate_offset() {
            return Err(anyhow::anyhow!("invalid save data: too short"));
        }

        let mut cursor = 0;
        cursor += size_of::<u16>(); // skip year
        cursor += size_of::<u8>(); // skip month
        cursor += size_of::<u8>(); // skip day
        cursor += size_of::<u8>(); // skip day_of_week
        cursor += size_of::<u8>(); // skip hour
        cursor += size_of::<u8>(); // skip minute

        // read string length
        let title_len = u16::from_le_bytes([buf[cursor], buf[cursor + 1]]) as usize;
        cursor += size_of::<u16>();
        cursor += title_len;
        let scene_title_len = u16::from_le_bytes([buf[cursor], buf[cursor + 1]]) as usize;
        cursor += size_of::<u16>();
        cursor += scene_title_len;
        let script_content_len = u16::from_le_bytes([buf[cursor], buf[cursor + 1]]) as usize;
        cursor += size_of::<u16>();
        cursor += script_content_len;

        let thumb_size = width * height * 4;
        // safely read thumb texture
        if buf.len() < cursor + thumb_size as usize {
            return Err(anyhow::anyhow!("invalid save data: too short"));
        }

        let thumb = buf[cursor..cursor + thumb_size as usize].to_vec();
        Ok(thumb)
    }

    fn calculate_offset() -> usize {
        let mut offset = 0;
        offset += 2; // year
        offset += 1; // month
        offset += 1; // day
        offset += 1; // day_of_week
        offset += 1; // hour
        offset += 1; // minute
        offset += 2; // title length (without null-terminated), aussme title is empty
        offset += 2; // scene_title length (without null-terminated), assume scene_title is empty
        offset += 2; // script_content length (without null-terminated), assume script_content is empty
        offset
    }

    fn read_string(buf: &[u8], len: u16, cursor: &mut usize, nls: Nls) -> Result<String> {
        let mut string = vec![];
        for _ in 0..len {
            let c = buf[*cursor];
            *cursor += 1;
            string.push(c);
        }

        match nls {
            Nls::ShiftJIS => {
                let string = encoding_rs::SHIFT_JIS.decode(&string).0;
                Ok(string.to_string())
            }
            Nls::GBK => {
                let string = encoding_rs::GBK.decode(&string).0;
                Ok(string.to_string())
            }
            Nls::UTF8 => {
                let string = encoding_rs::UTF_8.decode(&string).0;
                Ok(string.to_string())
            }
        }
    }

    pub fn load_from_mem(buf: &Vec<u8>, nls: Nls) -> Result<Self> {
        if buf.len() < Self::calculate_offset() {
            return Err(anyhow::anyhow!("invalid save data: too short"));
        }

        let mut cursor = 0;
        let year = u16::from_le_bytes([buf[cursor], buf[cursor + 1]]);
        cursor += 2;
        let month = buf[cursor];
        cursor += 1;
        let day = buf[cursor];
        cursor += 1;
        let day_of_week = buf[cursor];
        cursor += 1;
        let hour = buf[cursor];
        cursor += 1;
        let minute = buf[cursor];
        cursor += 1;

        let title_len = u16::from_le_bytes([buf[cursor], buf[cursor + 1]]) as usize;
        cursor += 2;
        let title = if title_len > 0 {
            Self::read_string(&buf, title_len as u16, &mut cursor, nls.clone())?
        } else {
            String::new()
        };

        let scene_title_len = u16::from_le_bytes([buf[cursor], buf[cursor + 1]]) as usize;
        cursor += 2;
        let scene_title = if scene_title_len > 0 {
            Self::read_string(&buf, scene_title_len as u16, &mut cursor, nls.clone())?
        } else {
            String::new()
        };

        let script_content_len = u16::from_le_bytes([buf[cursor], buf[cursor + 1]]) as usize;
        cursor += 2;
        let script_content = if script_content_len > 0 {
            Self::read_string(&buf, script_content_len as u16, &mut cursor, nls.clone())?
        } else {
            String::new()
        };

        Ok(SaveItem {
            year,
            month,
            day,
            day_of_week,
            hour,
            minute,
            title,
            scene_title,
            script_content,
        })
    }

    pub fn load_from_file(path: impl AsRef<Path>, nls: Nls) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let mut buf = vec![];
        reader.read_to_end(&mut buf)?;

        Self::load_from_mem(&buf, nls)
    }
}

#[derive(Debug, Default)]
pub struct SaveManager {
    thumb_width: u32,
    thumb_height: u32,
    current_scene_title: String,
    current_title: String,
    current_script_content: String,
    current_save_slot: u32,
    save_requested: bool,
    savedata_prepared: bool,
    should_load: bool,
    slots: Vec<Option<SaveItem>>,
}

impl SaveManager {
    pub fn new() -> Self {
        SaveManager {
            thumb_width: 0,
            thumb_height: 0,
            current_scene_title: String::new(),
            current_title: String::new(),
            current_script_content: String::new(),
            current_save_slot: 0,
            save_requested: false,
            savedata_prepared: false,
            should_load: false,
            slots: std::iter::repeat_with(|| Option::<SaveItem>::None)
                .take(1000)
                .collect::<Vec<_>>(),
        }
    }

    pub fn set_thumb_size(&mut self, width: u32, height: u32) {
        self.thumb_width = width;
        self.thumb_height = height;
    }

    pub fn set_current_scene_title(&mut self, title: String) {
        self.current_scene_title = title;
    }

    pub fn set_current_title(&mut self, title: String) {
        self.current_title = title;
    }

    pub fn set_current_script_content(&mut self, content: String) {
        self.current_script_content = content;
    }

    pub fn set_current_save_slot(&mut self, slot: u32) {
        self.current_save_slot = slot;
    }

    pub fn set_savedata_requested(&mut self, requested: bool) {
        self.save_requested = requested;
    }

    pub fn set_savedata_prepared(&mut self, prepared: bool) {
        self.savedata_prepared = prepared;
    }

    pub fn set_should_load(&mut self, should_load: bool) {
        self.should_load = should_load;
    }

    pub fn get_thumb_width(&self) -> u32 {
        self.thumb_width
    }

    pub fn get_thumb_height(&self) -> u32 {
        self.thumb_height
    }

    pub fn get_current_scene_title(&self) -> &str {
        &self.current_scene_title
    }

    pub fn get_current_title(&self) -> &str {
        &self.current_title
    }

    pub fn get_current_script_content(&self) -> &str {
        &self.current_script_content
    }

    pub fn get_current_save_slot(&self) -> u32 {
        self.current_save_slot
    }

    pub fn is_save_requested(&self) -> bool {
        self.save_requested
    }

    pub fn is_savedata_prepared(&self) -> bool {
        self.savedata_prepared
    }

    pub fn is_should_load(&self) -> bool {
        self.should_load
    }

    pub fn asynchronously_save(&mut self, slot: u32) {
        // mark the save status as dirty and perform the 'delayed' save
        self.current_save_slot = slot;
        self.save_requested = true;
    }

    pub fn test_save_slot(&self, slot: u32) -> bool {
        if let Some(save_item) = self.slots.get(slot as usize) {
            save_item.is_some()
        } else {
            false
        }
    }

    pub fn get_save_title(&self, slot: u32) -> String {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.title.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    pub fn get_save_scene_title(&self, slot: u32) -> String {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.scene_title.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    pub fn get_script_content(&self, slot: u32) -> String {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.script_content.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    pub fn get_year(&self, slot: u32) -> u16 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.year
            } else {
                u16::MAX
            }
        } else {
            u16::MAX
        }
    }

    pub fn get_month(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.month
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_day(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.day
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_day_of_week(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.day_of_week
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_hour(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.hour
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_minute(&self, slot: u32) -> u8 {
        if let Some(save_item) = self.slots.get(slot as usize) {
            if let Some(save_item) = save_item {
                save_item.minute
            } else {
                u8::MAX
            }
        } else {
            u8::MAX
        }
    }

    pub fn get_save_thumb(&self, slot: u32, width: u32, height: u32) -> Result<Vec<u8>> {
        SaveItem::read_thumb_texture(slot, width, height)
    }

    pub fn delete_savedata(&mut self, slot: u32) {
        if let Some(save_item) = self.slots.get_mut(slot as usize) {
            *save_item = None;
            app_base_path()
                .get_path()
                .join("save")
                .join(format!("s{}.bin", slot))
                .to_str()
                .map(std::fs::remove_file);
        }
    }

    pub fn copy_savedata(&mut self, src: u32, dst: u32) -> Result<()> {
        if let Some(save_item) = self.slots.get(src as usize) {
            if let Some(save_item) = save_item {
                self.slots[dst as usize] = Some(save_item.clone());
                let src_data = app_base_path()
                    .get_path()
                    .join("save")
                    .join(format!("s{}.bin", src));

                let dst_data = app_base_path()
                    .get_path()
                    .join("save")
                    .join(format!("s{}.bin", dst));

                let _ = std::fs::copy(src_data, dst_data)?;
            }
        }
        Ok(())
    }

    pub fn load_savedata(&mut self, slot: u32, nls: Nls) -> Result<()> {
        let _save_item = SaveItem::load_from_file(
            app_base_path()
                .get_path()
                .join("save")
                .join(format!("s{}.bin", slot)),
            nls,
        )?;
        Ok(())
    }

    pub fn load_save_buff(&mut self, slot: u32, nls: Nls, cache: &Vec<u8>) -> Result<()> {
        let save_item = SaveItem::load_from_mem(cache, nls)?;
        self.slots[slot as usize] = Some(save_item);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_save_item() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/s282.bin"));

        let save_item = SaveItem::load_from_file(filepath, Nls::ShiftJIS).unwrap();
        println!("{:?}", save_item);
    }
}
