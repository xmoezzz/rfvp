use std::vec;

#[derive(Debug, Clone, Default)]
pub struct TimerItem {
    enabled : bool,
    elapsed: u32,
    resolution: u32,
}

impl TimerItem {
    pub fn new() -> Self {
        Self {
            enabled: false,
            elapsed: 0,
            resolution: 0,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_elapsed(&mut self, elapsed: u32) {
        self.elapsed = elapsed;
    }

    pub fn set_resolution(&mut self, resolution: u32) {
        self.resolution = resolution;
    }

    pub fn get_enabled(&self) -> bool {
        self.enabled
    }

    pub fn get_elapsed(&self) -> u32 {
        self.elapsed
    }

    pub fn get_resolution(&self) -> u32 {
        self.resolution
    }
}

#[derive(Debug)]
pub struct TimerManager {
    items: Vec<TimerItem>,
    suspend: bool,
}

impl Default for TimerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerManager {
    pub fn new() -> Self {
        Self {
            items: vec![TimerItem::new(); 16],
            suspend: false,
        }
    }

    pub fn set_suspend(&mut self, suspend: bool) {
        self.suspend = suspend;
    }

    pub fn get_suspend(&self) -> bool {
        self.suspend
    }

    pub fn get_item(&mut self, index: usize) -> &mut TimerItem {
        &mut self.items[index]
    }

    pub fn set_enabled(&mut self, index: usize, enabled: bool) {
        self.items[index].set_enabled(enabled);
    }

    pub fn set_elapsed(&mut self, index: usize, elapsed: u32) {
        self.items[index].set_elapsed(elapsed);
    }

    pub fn set_resolution(&mut self, index: usize, resolution: u32) {
        self.items[index].set_resolution(resolution);
    }

    pub fn get_enabled(&self, index: usize) -> bool {
        self.items[index].get_enabled()
    }

    pub fn get_elapsed(&self, index: usize) -> u32 {
        self.items[index].get_elapsed()
    }

    pub fn get_resolution(&self, index: usize) -> u32 {
        self.items[index].get_resolution()
    }
}



