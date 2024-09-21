use std::fmt::Display;

use enum_map::{enum_map, EnumMap};
use glam::{vec2, Vec2};
use itertools::Itertools;
use petitset::PetitSet;
use winit::{
    event::{ElementState, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

use crate::{
    input::{action::UserInput, inputs::MouseButton},
    render::overlay::OverlayVisitable,
};

#[derive(Clone)]
pub struct RawInputState {
    /// Keyboard state, set of pressed keys
    pub keyboard: PetitSet<KeyCode, 16>,
    /// Mouse buttons state, simple state of each button
    pub mouse_buttons: EnumMap<MouseButton, bool>,
    pub mouse_position: Vec2,
    pub mouse_scroll_amount: f32,
    #[allow(unused)] // TODO: implement gamepad input
    gamepad: (),
    // TODO: mouse position?
    // How do we even handle mouse position?
}

impl RawInputState {
    pub fn new() -> Self {
        Self {
            keyboard: PetitSet::new(),
            mouse_buttons: enum_map! { _ => false },
            mouse_position: vec2(0.0, 0.0),
            mouse_scroll_amount: 0.0,
            gamepad: (),
        }
    }

    /// Returns the current state of the given button, and optionally the value (useful for axis)
    pub fn is_pressed(&self, input: &UserInput) -> Option<f32> {
        match input {
            UserInput::Keyboard(key_code) => self.keyboard.contains(key_code).then_some(1.0),
            UserInput::MouseButton(button) => self.mouse_buttons[*button].then_some(1.0),
            UserInput::GamepadButton(_) => None,
        }
    }

    // TODO: handle the sticks better?

    pub fn on_winit_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            self.keyboard.insert(keycode);
                        }
                        ElementState::Released => {
                            self.keyboard.remove(&keycode);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_position = vec2(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // press virtual mouse buttons
                // TODO: handle it in a smarter way or smth...
                let amount = match delta {
                    &winit::event::MouseScrollDelta::LineDelta(_x, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.y / 120.0) as f32, /* this value is windows-specific */
                };

                if amount > 0.0 {
                    self.mouse_buttons[MouseButton::WheelUp] = true;
                } else {
                    self.mouse_buttons[MouseButton::WheelDown] = true;
                }
                self.mouse_scroll_amount = amount;
            }
            &WindowEvent::MouseInput { button, state, .. } => {
                if let Some(button) = convert_winit_mouse_button(button) {
                    self.mouse_buttons[button] = match state {
                        ElementState::Pressed => true,
                        ElementState::Released => false,
                    }
                }
            }
            _ => {
                // don't care about other events
            }
        }
    }

    pub fn update(&mut self) {
        // NOTE: this should be done __after__ everything has handled the events
        self.mouse_scroll_amount = 0.0;
        self.mouse_buttons[MouseButton::WheelUp] = false;
        self.mouse_buttons[MouseButton::WheelDown] = false;
    }
}

impl Display for RawInputState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "RawInputState: {{")?;
        writeln!(
            f,
            "  keyboard: [{}]",
            self.keyboard.iter().map(|v| format!("{:?}", v)).join(", ")
        )?;
        writeln!(
            f,
            "  mouse_buttons: [{}]",
            self.mouse_buttons
                .iter()
                .filter_map(|(but, state)| state.then(|| format!("{:?}", but)))
                .join(", ")
        )?;
        writeln!(f, "}}")?;
        Ok(())
    }
}

impl OverlayVisitable for RawInputState {
    fn visit_overlay(&self, collector: &mut crate::render::overlay::OverlayCollector) {
        collector.overlay(
            "Input State",
            |_ctx, top_left| {
                top_left.label(format!(
                    "Input State: [{}] [{}]",
                    self.mouse_buttons
                        .iter()
                        .filter_map(|(but, state)| state.then(|| format!("{:?}", but)))
                        .join(", "),
                    self.keyboard.iter().map(|v| format!("{:?}", v)).join(", ")
                ));
            },
            true,
        );
    }
}

#[inline]
fn convert_winit_mouse_button(winit: winit::event::MouseButton) -> Option<MouseButton> {
    match winit {
        winit::event::MouseButton::Left => Some(MouseButton::Left),
        winit::event::MouseButton::Right => Some(MouseButton::Right),
        winit::event::MouseButton::Middle => Some(MouseButton::Middle),
        // TODO: how should we model those?
        winit::event::MouseButton::Back | winit::event::MouseButton::Forward => None,
        winit::event::MouseButton::Other(_) => None,
    }
}
