#![no_std]

pub const RFVP_SWITCH_CORE_ABI_VERSION: u32 = 2;

pub const RFVP_SWITCH_BUTTON_A: u32 = 1 << 0;
pub const RFVP_SWITCH_BUTTON_B: u32 = 1 << 1;
pub const RFVP_SWITCH_BUTTON_X: u32 = 1 << 2;
pub const RFVP_SWITCH_BUTTON_Y: u32 = 1 << 3;
pub const RFVP_SWITCH_BUTTON_L: u32 = 1 << 4;
pub const RFVP_SWITCH_BUTTON_R: u32 = 1 << 5;
pub const RFVP_SWITCH_BUTTON_ZL: u32 = 1 << 6;
pub const RFVP_SWITCH_BUTTON_ZR: u32 = 1 << 7;
pub const RFVP_SWITCH_BUTTON_PLUS: u32 = 1 << 8;
pub const RFVP_SWITCH_BUTTON_MINUS: u32 = 1 << 9;
pub const RFVP_SWITCH_BUTTON_UP: u32 = 1 << 10;
pub const RFVP_SWITCH_BUTTON_DOWN: u32 = 1 << 11;
pub const RFVP_SWITCH_BUTTON_LEFT: u32 = 1 << 12;
pub const RFVP_SWITCH_BUTTON_RIGHT: u32 = 1 << 13;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RfvpSwitchInputFrame {
    pub buttons_down: u32,
    pub buttons_up: u32,
    pub buttons_held: u32,
    pub touch_active: u32,
    pub touch_down: u32,
    pub touch_up: u32,
    pub touch_x: i32,
    pub touch_y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RfvpSwitchCoreStats {
    pub abi_version: u32,
    pub frame_no: u64,
    pub last_status: i32,
    pub forced_yield: u32,
    pub forced_yield_contexts: u32,
    pub main_thread_exited: u32,
    pub game_should_exit: u32,
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RfvpSwitchCoreStatus {
    Ok = 0,
    Null = -1,
    InvalidUtf8 = -2,
    InvalidNls = -3,
    LoadScriptFailed = -4,
    LoadVfsFailed = -5,
    VmTickFailed = -6,
}
