use crate::ipc::sf;
use crate::result::*;
use crate::version;

use super::CopyHandle;
use super::applet::AppletResourceUserId;

pub mod shmem;
use enum_iterator::{All, Sequence};
use nx_derive::{Request, Response};

define_bit_set! {
    DebugPadAttribute (u32) {
        IsConnected = bit!(0)
    }
}

define_bit_set! {
    DebugPadButton (u32) {
        A = bit!(0),
        B = bit!(1),
        X = bit!(2),
        Y = bit!(3),
        L = bit!(4),
        R = bit!(5),
        ZL = bit!(6),
        ZR = bit!(7),
        Start = bit!(8),
        Select = bit!(9),
        Left = bit!(10),
        Up = bit!(11),
        Right = bit!(12),
        Down = bit!(13)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct AnalogStickState {
    pub x: i32,
    pub y: i32,
}

define_bit_set! {
    TouchAttribute (u32) {
        None = 0,
        Start = bit!(0),
        End = bit!(1)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct TouchState {
    pub delta_time: u64,
    pub attributes: TouchAttribute,
    pub finger_id: u32,
    pub x: u32,
    pub y: u32,
    pub diameter_x: u32,
    pub diameter_y: u32,
    pub rotation_angle: u32,
    pub reserved: [u8; 4],
}

define_bit_set! {
    MouseAttribute (u32) {
        Transferable = bit!(0),
        IsConnected = bit!(1)
    }
}

define_bit_set! {
    MouseButton (u32) {
        Left = bit!(0),
        Right = bit!(1),
        Middle = bit!(2),
        Forward = bit!(3),
        Back = bit!(4)
    }
}

define_bit_set! {
    KeyboardModifier (u64) {
        Control = bit!(0),
        Shift = bit!(1),
        LeftAlt = bit!(2),
        RightAlt = bit!(3),
        Gui = bit!(4),
        CapsLock = bit!(8),
        ScrollLock = bit!(9),
        NumLock = bit!(10),
        Katakana = bit!(11),
        Hiragana = bit!(12)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct KeyboardKeyStates {
    pub key_bitfield: [u8; 0x20],
}

impl KeyboardKeyStates {
    /// Check if a particular key is depressed
    #[inline]
    pub fn is_down(&self, key: KeyboardKey) -> bool {
        let key_offset = key.bit_offset();
        (self.key_bitfield[key_offset >> 3] >> (key_offset & 7)) & 1 == 1
    }

    /// Check if a particular key is not depressed
    #[inline(always)]
    pub fn is_up(&self, key: KeyboardKey) -> bool {
        !self.is_down(key)
    }
}

pub struct KeyboardKeyDownIter(KeyboardKeyStates, All<KeyboardKey>);

impl Iterator for KeyboardKeyDownIter {
    type Item = KeyboardKey;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let key = self.1.next()?;
            let key_offset = key.bit_offset();
            if (self.0.key_bitfield[key_offset >> 3] >> (key_offset & 0b111)) & 1 == 1 {
                return Some(key);
            }
        }
    }
}

impl IntoIterator for KeyboardKeyStates {
    type Item = KeyboardKey;
    type IntoIter = KeyboardKeyDownIter;

    fn into_iter(self) -> Self::IntoIter {
        KeyboardKeyDownIter(self, enum_iterator::all::<Self::Item>())
    }
}

#[derive(Debug, Clone, Copy, Sequence)]
pub enum KeyboardKey {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D0,
    Return,
    Escape,
    Backspace,
    Tab,
    Space,
    Minus,
    Plus,
    OpenBracket,
    CloseBracket,
    Pipe,
    Tilde,
    Semicolon,
    Quote,
    Backquote,
    Comma,
    Period,
    Slash,
    CapsLock,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    PrintScreen,
    ScrollLock,
    Pause,
    Insert,
    Home,
    PageUp,
    Delete,
    End,
    PageDown,
    RightArrow,
    LeftArrow,
    DownArrow,
    UpArrow,
    NumLock,
    NumPadDivide,
    NumPadMultiply,
    NumPadSubtract,
    NumPadAdd,
    NumPadEnter,
    NumPad1,
    NumPad2,
    NumPad3,
    NumPad4,
    NumPad5,
    NumPad6,
    NumPad7,
    NumPad8,
    NumPad9,
    NumPad0,
    NumPadDot,
    Backslash,
    Application,
    Power,
    NumPadEquals,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    NumPadComma,
    Ro,
    KatakanaHiragana,
    Yen,
    Henkan,
    Muhenkan,
    NumPadCommaPc98,
    HangulEnglish,
    Hanja,
    Katakana,
    Hiragana,
    ZenkakuHankaku,
    LeftControl,
    LeftShift,
    LeftAlt,
    LeftGui,
    RightControl,
    RightShift,
    RightAlt,
    RightGui,
}

impl KeyboardKey {
    fn bit_offset(&self) -> usize {
        match *self {
            KeyboardKey::A => 4,
            KeyboardKey::B => 5,
            KeyboardKey::C => 6,
            KeyboardKey::D => 7,
            KeyboardKey::E => 8,
            KeyboardKey::F => 9,
            KeyboardKey::G => 10,
            KeyboardKey::H => 11,
            KeyboardKey::I => 12,
            KeyboardKey::J => 13,
            KeyboardKey::K => 14,
            KeyboardKey::L => 15,
            KeyboardKey::M => 16,
            KeyboardKey::N => 17,
            KeyboardKey::O => 18,
            KeyboardKey::P => 19,
            KeyboardKey::Q => 20,
            KeyboardKey::R => 21,
            KeyboardKey::S => 22,
            KeyboardKey::T => 23,
            KeyboardKey::U => 24,
            KeyboardKey::V => 25,
            KeyboardKey::W => 26,
            KeyboardKey::X => 27,
            KeyboardKey::Y => 28,
            KeyboardKey::Z => 29,
            KeyboardKey::D1 => 30,
            KeyboardKey::D2 => 31,
            KeyboardKey::D3 => 32,
            KeyboardKey::D4 => 33,
            KeyboardKey::D5 => 34,
            KeyboardKey::D6 => 35,
            KeyboardKey::D7 => 36,
            KeyboardKey::D8 => 37,
            KeyboardKey::D9 => 38,
            KeyboardKey::D0 => 39,
            KeyboardKey::Return => 40,
            KeyboardKey::Escape => 41,
            KeyboardKey::Backspace => 42,
            KeyboardKey::Tab => 43,
            KeyboardKey::Space => 44,
            KeyboardKey::Minus => 45,
            KeyboardKey::Plus => 46,
            KeyboardKey::OpenBracket => 47,
            KeyboardKey::CloseBracket => 48,
            KeyboardKey::Pipe => 49,
            KeyboardKey::Tilde => 50,
            KeyboardKey::Semicolon => 51,
            KeyboardKey::Quote => 52,
            KeyboardKey::Backquote => 53,
            KeyboardKey::Comma => 54,
            KeyboardKey::Period => 55,
            KeyboardKey::Slash => 56,
            KeyboardKey::CapsLock => 57,
            KeyboardKey::F1 => 58,
            KeyboardKey::F2 => 59,
            KeyboardKey::F3 => 60,
            KeyboardKey::F4 => 61,
            KeyboardKey::F5 => 62,
            KeyboardKey::F6 => 63,
            KeyboardKey::F7 => 64,
            KeyboardKey::F8 => 65,
            KeyboardKey::F9 => 66,
            KeyboardKey::F10 => 67,
            KeyboardKey::F11 => 68,
            KeyboardKey::F12 => 69,
            KeyboardKey::PrintScreen => 70,
            KeyboardKey::ScrollLock => 71,
            KeyboardKey::Pause => 72,
            KeyboardKey::Insert => 73,
            KeyboardKey::Home => 74,
            KeyboardKey::PageUp => 75,
            KeyboardKey::Delete => 76,
            KeyboardKey::End => 77,
            KeyboardKey::PageDown => 78,
            KeyboardKey::RightArrow => 79,
            KeyboardKey::LeftArrow => 80,
            KeyboardKey::DownArrow => 81,
            KeyboardKey::UpArrow => 82,
            KeyboardKey::NumLock => 83,
            KeyboardKey::NumPadDivide => 84,
            KeyboardKey::NumPadMultiply => 85,
            KeyboardKey::NumPadSubtract => 86,
            KeyboardKey::NumPadAdd => 87,
            KeyboardKey::NumPadEnter => 88,
            KeyboardKey::NumPad1 => 89,
            KeyboardKey::NumPad2 => 90,
            KeyboardKey::NumPad3 => 91,
            KeyboardKey::NumPad4 => 92,
            KeyboardKey::NumPad5 => 93,
            KeyboardKey::NumPad6 => 94,
            KeyboardKey::NumPad7 => 95,
            KeyboardKey::NumPad8 => 96,
            KeyboardKey::NumPad9 => 97,
            KeyboardKey::NumPad0 => 98,
            KeyboardKey::NumPadDot => 99,
            KeyboardKey::Backslash => 100,
            KeyboardKey::Application => 101,
            KeyboardKey::Power => 102,
            KeyboardKey::NumPadEquals => 103,
            KeyboardKey::F13 => 104,
            KeyboardKey::F14 => 105,
            KeyboardKey::F15 => 106,
            KeyboardKey::F16 => 107,
            KeyboardKey::F17 => 108,
            KeyboardKey::F18 => 109,
            KeyboardKey::F19 => 110,
            KeyboardKey::F20 => 111,
            KeyboardKey::F21 => 112,
            KeyboardKey::F22 => 113,
            KeyboardKey::F23 => 114,
            KeyboardKey::F24 => 115,
            KeyboardKey::NumPadComma => 133,
            KeyboardKey::Ro => 135,
            KeyboardKey::KatakanaHiragana => 136,
            KeyboardKey::Yen => 137,
            KeyboardKey::Henkan => 138,
            KeyboardKey::Muhenkan => 139,
            KeyboardKey::NumPadCommaPc98 => 140,
            KeyboardKey::HangulEnglish => 144,
            KeyboardKey::Hanja => 145,
            KeyboardKey::Katakana => 146,
            KeyboardKey::Hiragana => 147,
            KeyboardKey::ZenkakuHankaku => 148,
            KeyboardKey::LeftControl => 224,
            KeyboardKey::LeftShift => 225,
            KeyboardKey::LeftAlt => 226,
            KeyboardKey::LeftGui => 227,
            KeyboardKey::RightControl => 228,
            KeyboardKey::RightShift => 229,
            KeyboardKey::RightAlt => 230,
            KeyboardKey::RightGui => 231,
        }
    }
    pub fn get_ansi(&self) -> Option<&'static str> {
        match *self {
            Self::A => Some("a"),
            Self::B => Some("b"),
            Self::C => Some("c"),
            Self::D => Some("d"),
            Self::E => Some("e"),
            Self::F => Some("f"),
            Self::G => Some("g"),
            Self::H => Some("h"),
            Self::I => Some("i"),
            Self::J => Some("j"),
            Self::K => Some("k"),
            Self::L => Some("l"),
            Self::M => Some("m"),
            Self::N => Some("n"),
            Self::O => Some("o"),
            Self::P => Some("p"),
            Self::Q => Some("q"),
            Self::R => Some("r"),
            Self::S => Some("s"),
            Self::T => Some("t"),
            Self::U => Some("u"),
            Self::V => Some("v"),
            Self::W => Some("w"),
            Self::X => Some("x"),
            Self::Y => Some("y"),
            Self::Z => Some("z"),
            Self::Return => Some("\r\n"),
            Self::Backquote => Some("`"),
            Self::Backslash => Some("\\"),
            Self::Comma => Some(","),
            Self::CloseBracket => Some(")"),
            Self::OpenBracket => Some("("),
            Self::DownArrow => Some("\x1b[(224;80)"),
            Self::UpArrow => Some("\x1b[(224;72)"),
            Self::Minus => Some("-"),
            Self::Plus => Some("+"),
            Self::LeftArrow => Some("\x1B[(224;75)"),
            Self::RightArrow => Some("\x1B[(224;77)"),
            Self::F1 => Some("\x1b[0;59"),
            Self::F2 => Some("\x1b[0;60"),
            Self::F3 => Some("\x1b[0;61"),
            Self::F4 => Some("\x1b[0;62"),
            Self::F5 => Some("\x1b[0;63"),
            Self::F6 => Some("\x1b[0;64"),
            Self::F7 => Some("\x1b[0;65"),
            Self::F8 => Some("\x1b[0;66"),
            Self::F9 => Some("\x1b[0;67"),
            Self::F10 => Some("\x1b[0;68"),
            Self::F11 => Some("\x1b[0;133"),
            Self::F12 => Some("\x1b[0;134"),
            Self::Home => Some("\x1b[(224;71)"),
            Self::End => Some("\x1b[(224;79)"),
            // TODO: More - https://gist.github.com/fnky/458719343aabd01cfb17a3a4f7296797
            _ => None,
        }
    }
}

define_bit_set! {
    BasicXpadAttribute (u32) {
        // TODO: are these known at all?
    }
}

define_bit_set! {
    BasicXpadButton (u32) {
        // TODO: are these known at all?
    }
}

define_bit_set! {
    DigitizerAttribute (u32) {
        // TODO: are these known at all?
    }
}

define_bit_set! {
    DigitizerButton (u32) {
        // TODO: are these known at all?
    }
}

define_bit_set! {
    HomeButton (u32) {
        // TODO: are these known at all?
    }
}

define_bit_set! {
    SleepButton (u32) {
        // TODO: are these known at all?
    }
}

define_bit_set! {
    CaptureButton (u32) {
        // TODO: are these known at all?
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct InputSourceState {
    pub timestamp: u64,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum UniquePadType {
    Embedded = 0,
    FullKeyController = 1,
    RightController = 2,
    LeftController = 3,
    DebugPadController = 4,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum UniquePadInterface {
    Embedded = 0,
    Rail = 1,
    Bluetooth = 2,
    Usb = 3,
}

pub type UniquePadSerialNumber = [u8; 0x10];

define_bit_set! {
    AnalogStickCalibrationFlags (u32) {
        // TODO: are these known at all?
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum AnalogStickManualCalibrationStage {
    ReleaseFromRight = 0,
    ReleaseFromBottom = 1,
    ReleaseFromLeft = 2,
    ReleaseFromTop = 3,
    Rotate = 4,
    Update = 5,
    Completed = 6,
    Clear = 7,
    ClearCompleted = 8,
}

define_bit_set! {
    SixAxisSensorUserCalibrationFlags (u32) {
        // TODO: are these known at all?
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum SixAxisSensorUserCalibrationStage {
    Measuring = 0,
    Update = 1,
    Completed = 2,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum GestureType {
    Idle = 0,
    Complete = 1,
    Cancel = 2,
    Touch = 3,
    Press = 4,
    Tap = 5,
    Pan = 6,
    Swipe = 7,
    Pinch = 8,
    Rotate = 9,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum GestureDirection {
    None = 0,
    Left = 1,
    Up = 2,
    Right = 3,
    Down = 4,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct GesturePoint {
    pub x: u32,
    pub y: u32,
}

define_bit_set! {
    GestureAttribute (u32) {
        IsNewTouch = bit!(4),
        IsDoubleTap = bit!(8)
    }
}

define_bit_set! {
    NpadStyleTag (u32) {
        FullKey = bit!(0), // Pro controller
        Handheld = bit!(1), // Joy-Con controller in handheld mode
        JoyDual = bit!(2), // Joy-Con controller in dual mode
        JoyLeft = bit!(3), // Joy-Con left controller in single mode
        JoyRight = bit!(4), // Joy-Con right controller in single mode
        Gc = bit!(5), // GameCube controller
        Palma = bit!(6), // Poké Ball Plus controller
        Lark = bit!(7), // NES/Famicom controller
        HandheldLark = bit!(8), // NES/Famicom controller (handheld)
        Lucia = bit!(9), // SNES controller
        Lagon = bit!(10), // N64 controller
        Lager = bit!(11), // Sega Genesis controller
        SystemExt = bit!(29), // Generic external controller
        System = bit!(30) // Generic controller
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(i64)]
pub enum NpadJoyDeviceType {
    Left = 0,
    Right = 1,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum NpadIdType {
    No1 = 0, // Players 1-8
    No2 = 1,
    No3 = 2,
    No4 = 3,
    No5 = 4,
    No6 = 5,
    No7 = 6,
    No8 = 7,
    Other = 0x10,
    Handheld = 0x20,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum NpadJoyAssignmentMode {
    Dual = 0,
    Single = 1,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ColorAttribute {
    Ok = 0,
    ReadError = 1,
    NoController = 2,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadControllerColor {
    pub main: u32,
    pub sub: u32,
}

define_bit_set! {
    NpadButton (u64) {
        A = bit!(0),
        B = bit!(1),
        X = bit!(2),
        Y = bit!(3),
        StickL = bit!(4),
        StickR = bit!(5),
        L = bit!(6),
        R = bit!(7),
        ZL = bit!(8),
        ZR = bit!(9),
        Plus = bit!(10),
        Minus = bit!(11),
        Left = bit!(12),
        Up = bit!(13),
        Right = bit!(14),
        Down = bit!(15),
        StickLLeft = bit!(16),
        StickLUp = bit!(17),
        StickLRight = bit!(18),
        StickLDown = bit!(19),
        StickRLeft = bit!(20),
        StickRUp = bit!(21),
        StickRRight = bit!(22),
        StickRDown = bit!(23),
        LeftSL = bit!(24),
        LeftSR = bit!(25),
        RightSL = bit!(26),
        RightSR = bit!(27),
        Palma = bit!(28),
        Verification = bit!(29),
        HandheldLeftB = bit!(30),
        LagonCLeft = bit!(31),
        LagonCUp = bit!(32),
        LagonCRight = bit!(33),
        LagonCDown = bit!(34)
    }
}

define_bit_set! {
    NpadAttribute (u32) {
        IsConnected = bit!(0),
        IsWired = bit!(1),
        IsLeftConnected = bit!(2),
        IsLeftWired = bit!(3),
        IsRightConnected = bit!(4),
        IsRightWired = bit!(5)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct DirectionState {
    pub xx: u32,
    pub xy: u32,
    pub xz: u32,
    pub yx: u32,
    pub yy: u32,
    pub yz: u32,
    pub zx: u32,
    pub zy: u32,
    pub zz: u32,
}

define_bit_set! {
    SixAxisSensorAttribute (u32) {
        IsConnected = bit!(0),
        IsInterpolated = bit!(1)
    }
}

define_bit_set! {
    DeviceType (u32) {
        FullKey = bit!(0),
        DebugPad = bit!(1),
        HandheldLeft = bit!(2),
        HandheldRight = bit!(3),
        JoyLeft = bit!(4),
        JoyRight = bit!(5),
        Palma = bit!(6),
        LarkHvcLeft = bit!(7),
        LarkHvcRight = bit!(8),
        LarkNesLeft = bit!(9),
        LarkNesRight = bit!(10),
        HandheldLarkHvcLeft = bit!(11),
        HandheldLarkHvcRight = bit!(12),
        HandheldLarkNesLeft = bit!(13),
        HandheldLarkNesRight = bit!(14),
        Lucia = bit!(15),
        Lagon = bit!(16),
        Lager = bit!(17),
        System = bit!(31)
    }
}

define_bit_set! {
    NpadSystemProperties (u64) {
        IsChargingJoyDual = bit!(0),
        IsChargingJoyLeft = bit!(1),
        IsChargingJoyRight = bit!(2),
        IsPoweredJoyDual = bit!(3),
        IsPoweredJoyLeft = bit!(4),
        IsPoweredJoyRight = bit!(5),
        IsUnsuportedButtonPressedOnNpadSystem = bit!(9),
        IsUnsuportedButtonPressedOnNpadSystemExt = bit!(10),
        IsAbxyButtonOriented = bit!(11),
        IsSlSrButtonOriented = bit!(12),
        IsPlusAvailable = bit!(13),
        IsMinusAvailable = bit!(14),
        IsDirectionalButtonsAvailable = bit!(15)
    }
}

define_bit_set! {
    NpadSystemButtonProperties (u32) {
        IsUnintendedHomeButtonInputProtectionEnabled = bit!(0)
    }
}

pub type NpadBatteryLevel = u32;

define_bit_set! {
    AppletFooterUiAttribute (u32) {
        // TODO: are these known at all?
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum AppletFooterUiType {
    None = 0,
    HandheldNone = 1,
    HandheldJoyConLeftOnly = 2,
    HandheldJoyConRightOnly = 3,
    HandheldJoyConLeftJoyConRight = 4,
    JoyDual = 5,
    JoyDualLeftOnly = 6,
    JoyDualRightOnly = 7,
    JoyLeftHorizontal = 8,
    JoyLeftVertical = 9,
    JoyRightHorizontal = 10,
    JoyRightVertical = 11,
    SwitchProController = 12,
    CompatibleProController = 13,
    CompatibleJoyCon = 14,
    LarkHvc1 = 15,
    LarkHvc2 = 16,
    LarkNesLeft = 17,
    LarkNesRight = 18,
    Lucia = 19,
    Verification = 20,
    Lagon = 21,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum NpadLarkType {
    Invalid = 0,
    H1 = 1,
    H2 = 2,
    NL = 3,
    NR = 4,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum NpadLuciaType {
    Invalid = 0,
    J = 1,
    E = 2,
    U = 3,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum NpadLagerType {
    Invalid = 0,
    J = 1,
    E = 2,
    U = 3,
}

define_bit_set! {
    SixAxisSensorProperties (u8) {
        IsSixAxisSensorDeviceNewlyAssigned = bit!(0),
        IsFirmwareUpdateAvailableForSixAxisSensor = bit!(1)
    }
}

define_bit_set! {
    LockKeyFlags (u32) {
        NumLockOn = bit!(0),
        NumLockOff = bit!(1),
        NumLockToggle = bit!(2),
        CapsLockOn = bit!(3),
        CapsLockOff = bit!(4),
        CapsLockToggle = bit!(5),
        ScrollLockOn = bit!(6),
        ScrollLockOff = bit!(7),
        ScrollLockToggle = bit!(8)
    }
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait AppletResource {
    #[ipc_rid(0)]
    fn get_shared_memory_handle(&mut self) -> sf::CopyHandle;
}

#[nx_derive::ipc_trait]
pub trait Hid {
    #[ipc_rid(0)]
    #[return_session]
    fn create_applet_resource(&mut self, aruid: AppletResourceUserId) -> AppletResource;
    #[ipc_rid(100)]
    fn set_supported_npad_style_set(
        &mut self,
        npad_style_tag: NpadStyleTag,
        aruid: AppletResourceUserId,
    );
    #[ipc_rid(101)]
    fn get_supported_npad_style_set(&self, aruid: AppletResourceUserId) -> NpadStyleTag;
    #[ipc_rid(102)]
    fn set_supported_npad_id_type(
        &mut self,
        aruid: AppletResourceUserId,
        npad_ids: sf::InPointerBuffer<'_, NpadIdType>,
    );
    #[ipc_rid(103)]
    fn activate_npad(&mut self, aruid: AppletResourceUserId);
    #[ipc_rid(104)]
    fn deactivate_npad(&mut self, aruid: AppletResourceUserId);
    #[ipc_rid(109)]
    fn activate_npad_with_revision(&mut self, revision: i32, aruid: AppletResourceUserId);
    #[ipc_rid(123)]
    fn set_npad_joy_assignment_mode_single(
        &mut self,
        npad_id: NpadIdType,
        aruid: AppletResourceUserId,
        joy_type: NpadJoyDeviceType,
    );
    #[ipc_rid(124)]
    fn set_npad_joy_assignment_mode_dual(
        &mut self,
        npad_id: NpadIdType,
        aruid: AppletResourceUserId,
    );
}

#[nx_derive::ipc_trait]
pub trait HidSys {
    #[ipc_rid(31)]
    fn send_keyboard_lock_key_event(&self, flags: LockKeyFlags);
    #[ipc_rid(101)]
    fn acquire_home_button_event_handle(&self, aruid: AppletResourceUserId) -> CopyHandle;
    #[ipc_rid(111)]
    fn activate_home_button(&self, aruid: AppletResourceUserId);
    #[ipc_rid(121)]
    fn acquire_sleep_button_event_handle(&self, aruid: AppletResourceUserId) -> CopyHandle;
    #[ipc_rid(131)]
    fn activate_sleep_button(&self, aruid: AppletResourceUserId);
    #[ipc_rid(141)]
    fn acquire_capture_button_event_handle(&self, aruid: AppletResourceUserId) -> CopyHandle;
    #[ipc_rid(151)]
    fn activate_capture_button(&self, aruid: AppletResourceUserId);
}
