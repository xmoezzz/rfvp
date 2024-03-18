use std::{io::Read, path::Path};
use anyhow::Result;
use chrono::{DateTime, Datelike, Local, Timelike, Weekday};

use crate::script::parser::Nls;


pub enum SaveDataFunction {
    LoadSaveThumbToTexture = 0,
    TestSaveData = 1,
    DeleteSaveData = 2,
    CopySaveData = 3,
}

#[derive(Debug)]
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
            },
            Nls::GBK => {
                let string = encoding_rs::GBK.decode(&string).0;
                Ok(string.to_string())
            },
            Nls::UTF8 => {
                let string = encoding_rs::UTF_8.decode(&string).0;
                Ok(string.to_string())
            },
        }
    }

    pub fn load_from_file(path: impl AsRef<Path>, nls: Nls) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let mut buf = vec![];
        reader.read_to_end(&mut buf)?;

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
        } else  {
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

}


#[derive(Debug, Default)]
pub struct SaveManager {
    pub thumb_width: u32,
    pub thumb_height: u32,
    pub current_scene_title: String,
    pub current_title: String,
    pub current_script_content: String,
    pub current_save_slot: u32,
    pub save_requested: bool,
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

    pub fn asynchronously_save(&mut self, slot: u32) {
        // mark the save status as dirty and perform the 'delayed' save
        self.current_save_slot = slot;
        self.save_requested = true;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_save_item() {
        let filepath = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/s282.bin"
        ));

        let save_item = SaveItem::load_from_file(filepath, Nls::ShiftJIS).unwrap();
        println!("{:?}", save_item);
    }
}