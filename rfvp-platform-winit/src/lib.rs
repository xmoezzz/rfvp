use rfvp_input::{ButtonState, InputEvent, InputHub, KeyCode};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, NamedKey};

pub fn handle_window_event(hub: &InputHub, ev: &WindowEvent) {
    if let Some(evs) = translate_window_event(ev) {
        hub.push_many(evs);
    }
}

pub fn translate_window_event(ev: &WindowEvent) -> Option<Vec<InputEvent>> {
    match ev {
        WindowEvent::KeyboardInput { event, .. } => {
            let (code, repeat) = match &event.logical_key {
                Key::Named(nk) => (map_named_key(*nk)?, event.repeat),
                _ => return None,
            };
            let state = match event.state {
                ElementState::Pressed => ButtonState::Pressed,
                ElementState::Released => ButtonState::Released,
            };

            let mut out = Vec::with_capacity(2);
            out.push(InputEvent::Key { code, state, repeat });

            if let Some(text) = event.text.as_deref() {
                if !text.is_empty() && matches!(event.state, ElementState::Pressed) {
                    out.push(InputEvent::Text { utf8: text.to_string() });
                }
            }
            Some(out)
        }

        WindowEvent::CursorMoved { position, .. } => Some(vec![InputEvent::CursorMove {
            x: position.x as i32,
            y: position.y as i32,
        }]),

        WindowEvent::CursorEntered { .. } => Some(vec![InputEvent::CursorIn(true)]),
        WindowEvent::CursorLeft { .. } => Some(vec![InputEvent::CursorIn(false)]),

        WindowEvent::MouseWheel { delta, .. } => {
            let v = match delta {
                MouseScrollDelta::LineDelta(_x, y) => (*y * 120.0) as i32,
                MouseScrollDelta::PixelDelta(p) => p.y as i32,
            };
            Some(vec![InputEvent::Wheel { delta: v }])
        }

        WindowEvent::MouseInput { state, button, .. } => {
            let state = match state {
                ElementState::Pressed => ButtonState::Pressed,
                ElementState::Released => ButtonState::Released,
            };
            let code = match button {
                MouseButton::Left => KeyCode::MouseLeft,
                MouseButton::Right => KeyCode::MouseRight,
                _ => return None,
            };
            Some(vec![InputEvent::Key { code, state, repeat: false }])
        }

        WindowEvent::Focused(f) => Some(vec![InputEvent::Focused(*f)]),

        _ => None,
    }
}

fn map_named_key(nk: NamedKey) -> Option<KeyCode> {
    Some(match nk {
        NamedKey::Shift => KeyCode::Shift,
        NamedKey::Control => KeyCode::Ctrl,
        NamedKey::Enter => KeyCode::Enter,
        NamedKey::Escape => KeyCode::Esc,
        NamedKey::Space => KeyCode::Space,
        NamedKey::Tab => KeyCode::Tab,
        NamedKey::ArrowUp => KeyCode::UpArrow,
        NamedKey::ArrowDown => KeyCode::DownArrow,
        NamedKey::ArrowLeft => KeyCode::LeftArrow,
        NamedKey::ArrowRight => KeyCode::RightArrow,
        NamedKey::F1 => KeyCode::F1,
        NamedKey::F2 => KeyCode::F2,
        NamedKey::F3 => KeyCode::F3,
        NamedKey::F4 => KeyCode::F4,
        NamedKey::F5 => KeyCode::F5,
        NamedKey::F6 => KeyCode::F6,
        NamedKey::F7 => KeyCode::F7,
        NamedKey::F8 => KeyCode::F8,
        NamedKey::F9 => KeyCode::F9,
        NamedKey::F10 => KeyCode::F10,
        NamedKey::F11 => KeyCode::F11,
        NamedKey::F12 => KeyCode::F12,
        _ => return None,
    })
}
