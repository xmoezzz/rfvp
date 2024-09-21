use enum_map::Enum;
pub use winit::keyboard::KeyCode;

#[allow(unused)] // It will be used... eventually
#[derive(Debug, Hash, Ord, PartialOrd, PartialEq, Eq, Clone, Copy)]
pub enum GamepadAxisType {
    LeftStickX,
    LeftStickY,
    LeftZ,
    RightStickX,
    RightStickY,
    RightZ,
    // Other(u8),
}

#[allow(unused)] // It will be used... eventually
#[derive(Debug, Hash, Ord, PartialOrd, PartialEq, Eq, Clone, Copy)]
pub enum GamepadButtonType {
    South,
    East,
    North,
    West,
    C,
    Z,
    LeftTrigger,
    LeftTrigger2,
    RightTrigger,
    RightTrigger2,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    // Other(u8),
}

#[derive(Debug, Hash, Ord, PartialOrd, PartialEq, Eq, Clone, Copy, Enum)]
pub enum MouseButton {
    /// Left mouse button
    Left,
    /// Right mouse button
    Right,
    /// Middle mouse button (pressing the scroll wheel)
    Middle,
    /// Wheel up pseudo-button (scrolling up, discrete)
    WheelUp,
    /// Wheel down pseudo-button (scrolling down, discrete)
    WheelDown,
    // Ignore "other" mouse buttons for the sake of simplicity
    // Other(u16),
}
